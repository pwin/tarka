//! The row backend: records in, triples out, one record at a time.
//!
//! Records are read in batches. Each batch is lifted (column conversions natively,
//! a SPARQL WHERE clause with spareval) and shaped on its own thread, and the triples
//! are written in input order. Blank node labels come from a record's position in
//! the input, so the output does not depend on how the work was split.

use std::io;

use oxrdf::{Literal, Triple};
use rayon::prelude::*;
use tarka_core::{Cell, CellSource, ColumnBinding, Emitter, Labels, Lifting, Plan, Record, Value, VarId};
use tarka_io::TripleSink;
use tarka_tarql::{EvalError, SparqlEvaluator};
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct RunOptions {
    /// Records per batch. A query with solution modifiers runs one record at a time.
    pub batch_size: usize,
    /// Batches to work on at once (0: one per CPU).
    pub jobs: usize,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self { batch_size: 512, jobs: 0 }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunStats {
    /// Records read (a split row counts once per part).
    pub records: u64,
    /// Solutions of the lifting (usually one per record).
    pub solutions: u64,
    /// Triples made, before duplicates are removed.
    pub triples: u64,
}

#[derive(Debug, Error)]
pub enum RunError {
    #[error("{0}")]
    Input(Box<dyn std::error::Error + Send + Sync>),
    #[error("the mapping cannot be evaluated: {0}")]
    Check(EvalError),
    #[error("{0}")]
    Eval(#[from] EvalError),
    #[error("{0}")]
    Output(#[from] io::Error),
    #[error("{0}")]
    Mapping(String),
}

/// How one plan's lifting is evaluated over records with known columns.
enum Lifter {
    Columns(Vec<(VarId, Option<usize>, ColumnBinding)>),
    Sparql(Box<SparqlEvaluator>, Vec<VarId>),
}

impl Lifter {
    fn new(plan: &Plan, columns: &[String]) -> Result<Self, RunError> {
        match &plan.lifting {
            Lifting::Given => Err(RunError::Mapping(format!("{} takes its values as OTTR instances, not from a table", plan.name))),
            Lifting::Columns(bindings) => {
                let mut out = Vec::new();
                for b in bindings {
                    let index = match &b.source {
                        CellSource::RowNumber => None,
                        CellSource::Column(c) => match columns.iter().position(|x| x == c) {
                            Some(i) => Some(i),
                            // a column that is not there is always empty
                            None => Some(usize::MAX),
                        },
                    };
                    out.push((b.var, index, b.clone()));
                }
                Ok(Self::Columns(out))
            }
            Lifting::Sparql(lifting) => {
                let evaluator = SparqlEvaluator::new(lifting, &plan.prefixes, columns);
                evaluator.check().map_err(RunError::Check)?;
                Ok(Self::Sparql(Box::new(evaluator), lifting.outputs.iter().map(|(v, _)| *v).collect()))
            }
        }
    }

    fn per_row(&self) -> bool {
        matches!(self, Self::Sparql(e, _) if e.per_row())
    }

    /// The solutions (environments) of each record.
    fn lift(&self, plan: &Plan, records: &[Record], first: u64) -> Result<Vec<Vec<Vec<Option<Value>>>>, RunError> {
        let empty = || vec![None; plan.vars.len()];
        match self {
            Self::Columns(bindings) => records
                .iter()
                .map(|r| {
                    let mut env = empty();
                    for (var, index, b) in bindings {
                        env[var.0] = match index {
                            None => Some(Value::Term(Literal::from(r.row as i64).into())),
                            Some(i) => match r.cells.get(*i).and_then(Option::as_ref) {
                                Some(cell) => convert(cell, b, plan)?,
                                None => None,
                            },
                        };
                    }
                    Ok(vec![env])
                })
                .collect(),
            Self::Sparql(evaluator, outputs) => Ok(evaluator
                .evaluate(records, first)?
                .into_iter()
                .map(|solutions| {
                    solutions
                        .into_iter()
                        .map(|solution| {
                            let mut env = empty();
                            for (var, value) in outputs.iter().zip(solution) {
                                env[var.0] = value.map(Value::Term);
                            }
                            env
                        })
                        .collect()
                })
                .collect()),
        }
    }
}

/// A cell's value for a column binding: text converted by type, or for a list parameter,
/// split on the binding's separator; a list cell converted item by item.
pub fn convert(cell: &Cell, b: &ColumnBinding, plan: &Plan) -> Result<Option<Value>, RunError> {
    let items = |parts: &mut dyn Iterator<Item = &str>| {
        Value::List(
            parts
                .filter(|part| !part.trim().is_empty())
                .filter_map(|part| b.conversion.apply(part, &plan.prefixes).map(Value::Term))
                .collect(),
        )
    };
    let column = || match &b.source {
        CellSource::Column(c) => c.as_str(),
        CellSource::RowNumber => "ROWNUM",
    };
    Ok(match (cell, b.list) {
        (Cell::Text(text), false) => b.conversion.apply(text, &plan.prefixes).map(Value::Term),
        (Cell::Text(text), true) => match &b.list_separator {
            Some(sep) => Some(items(&mut text.split(sep.as_str()))),
            None => {
                let c = column();
                return Err(RunError::Mapping(format!(
                    "parameter ?{c} has a list type: give the separator of its text cells (--list {c} ';', or lists={{\"{c}\": \";\"}} in Python)"
                )));
            }
        },
        (Cell::List(list), true) => Some(items(&mut list.iter().map(String::as_str))),
        (Cell::List(_), false) => {
            return Err(RunError::Mapping(format!("column {} holds lists, but its parameter does not take a list", column())));
        }
    })
}

/// Runs `plan` over `records` (whose cells follow `columns`) and writes the triples to
/// `sink`, one record's worth at a time.
pub fn run<E>(
    plan: &Plan,
    columns: &[String],
    records: impl IntoIterator<Item = Result<Record, E>>,
    sink: &mut dyn TripleSink,
    options: &RunOptions,
) -> Result<RunStats, RunError>
where
    E: std::error::Error + Send + Sync + 'static,
{
    let lifter = Lifter::new(plan, columns)?;
    let batch_size = if lifter.per_row() { 1 } else { options.batch_size.max(1) };
    let jobs = if options.jobs == 0 { rayon::current_num_threads() } else { options.jobs };
    let mut stats = RunStats::default();
    let mut records = records.into_iter();
    loop {
        // read up to `jobs` batches, shape them in parallel, write them in order
        let mut batches: Vec<(u64, Vec<Record>)> = Vec::with_capacity(jobs);
        for _ in 0..jobs {
            let mut batch = Vec::with_capacity(batch_size);
            for r in records.by_ref().take(batch_size) {
                batch.push(r.map_err(|e| RunError::Input(Box::new(e)))?);
            }
            if batch.is_empty() {
                break;
            }
            batches.push((stats.records, batch));
            stats.records += batches.last().map_or(0, |(_, b)| b.len() as u64);
        }
        if batches.is_empty() {
            break;
        }
        let shaped: Vec<Result<Shaped, RunError>> = batches.par_iter().map(|(first, batch)| shape(plan, &lifter, batch, *first)).collect();
        for result in shaped {
            let (solutions, rows) = result?;
            stats.solutions += solutions;
            for triples in rows {
                stats.triples += triples.len() as u64;
                sink.row(triples)?;
            }
        }
    }
    sink.finish()?;
    Ok(stats)
}

/// A shaped batch: the number of solutions, and each record's triples.
type Shaped = (u64, Vec<Vec<Triple>>);

/// Lifts and shapes one batch.
fn shape(plan: &Plan, lifter: &Lifter, batch: &[Record], first: u64) -> Result<Shaped, RunError> {
    let mut emitter = Emitter::new(plan);
    let mut labels = Labels::new("b");
    let mut solutions = 0;
    let mut rows = Vec::with_capacity(batch.len());
    for (i, envs) in lifter.lift(plan, batch, first)?.into_iter().enumerate() {
        labels.reset(format!("b{}x", first + i as u64));
        let mut triples = Vec::new();
        for mut env in envs {
            solutions += 1;
            emitter.emit(&mut env, &mut labels, &mut |t| triples.push(t));
        }
        rows.push(triples);
    }
    Ok((solutions, rows))
}
