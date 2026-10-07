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
    match cell {
        Cell::Text(text) => convert_text(text, b, plan),
        Cell::List(list) => convert_items(list.iter().map(String::as_str), b, plan),
    }
}

fn column_of(b: &ColumnBinding) -> &str {
    match &b.source {
        CellSource::Column(c) => c.as_str(),
        CellSource::RowNumber => "ROWNUM",
    }
}

fn list_value<'a>(items: impl IntoIterator<Item = &'a str>, b: &ColumnBinding, plan: &Plan) -> Value {
    Value::List(
        items
            .into_iter()
            .filter(|part| !part.trim().is_empty())
            .filter_map(|part| b.conversion.apply(part, &plan.prefixes).map(Value::Term))
            .collect(),
    )
}

/// A text cell's value for a column binding (see [`convert`]).
pub fn convert_text(text: &str, b: &ColumnBinding, plan: &Plan) -> Result<Option<Value>, RunError> {
    if !b.list {
        return Ok(b.conversion.apply(text, &plan.prefixes).map(Value::Term));
    }
    match &b.list_separator {
        Some(sep) => Ok(Some(list_value(text.split(sep.as_str()), b, plan))),
        None => {
            let c = column_of(b);
            Err(RunError::Mapping(format!(
                "parameter ?{c} has a list type: give the separator of its text cells (--list {c} ';', or lists={{\"{c}\": \";\"}} in Python)"
            )))
        }
    }
}

/// A list cell's value for a column binding, from its items (see [`convert`]).
pub fn convert_items<'a>(items: impl IntoIterator<Item = &'a str>, b: &ColumnBinding, plan: &Plan) -> Result<Option<Value>, RunError> {
    if !b.list {
        return Err(RunError::Mapping(format!("column {} holds lists, but its parameter does not take a list", column_of(b))));
    }
    Ok(Some(list_value(items, b, plan)))
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
    let batch_size = if lifter.per_row() { 1 } else { options.batch_size };
    let records = records.into_iter().map(|r| r.map_err(|e| RunError::Input(Box::new(e))));
    drive(records, batch_size, options.jobs, sink, |batch, first| {
        let envs = lifter.lift(plan, &batch, first)?;
        Ok(emit(plan, envs, first))
    })
}

/// Runs `plan` over rows whose values are made already, one environment per row (a
/// value or `None` for each of the plan's variables, by `VarId`): the shape layer only,
/// for a backend that lifts by itself.
pub fn run_envs<E>(
    plan: &Plan,
    envs: impl IntoIterator<Item = Result<Vec<Option<Value>>, E>>,
    sink: &mut dyn TripleSink,
    options: &RunOptions,
) -> Result<RunStats, RunError>
where
    E: Into<RunError>,
{
    let envs = envs.into_iter().map(|e| {
        e.map_err(Into::into).map(|mut env| {
            env.resize(plan.vars.len(), None);
            env
        })
    });
    drive(envs, options.batch_size, options.jobs, sink, |batch, first| {
        Ok(emit(plan, batch.into_iter().map(|env| vec![env]).collect(), first))
    })
}

/// Reads up to `jobs` batches at a time, shapes them in parallel, and writes them in order.
fn drive<T: Send>(
    mut items: impl Iterator<Item = Result<T, RunError>>,
    batch_size: usize,
    jobs: usize,
    sink: &mut dyn TripleSink,
    shape: impl Fn(Vec<T>, u64) -> Result<Shaped, RunError> + Sync,
) -> Result<RunStats, RunError> {
    let batch_size = batch_size.max(1);
    let jobs = if jobs == 0 { rayon::current_num_threads() } else { jobs };
    let mut stats = RunStats::default();
    loop {
        let mut batches: Vec<(u64, Vec<T>)> = Vec::with_capacity(jobs);
        for _ in 0..jobs {
            let mut batch = Vec::with_capacity(batch_size);
            for r in items.by_ref().take(batch_size) {
                batch.push(r?);
            }
            if batch.is_empty() {
                break;
            }
            let n = batch.len() as u64;
            batches.push((stats.records, batch));
            stats.records += n;
        }
        if batches.is_empty() {
            break;
        }
        let shaped: Vec<Result<Shaped, RunError>> = batches.into_par_iter().map(|(first, batch)| shape(batch, first)).collect();
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

/// Shapes one batch: each record's solutions (environments) into its triples.
fn emit(plan: &Plan, records: Vec<Vec<Vec<Option<Value>>>>, first: u64) -> Shaped {
    let mut emitter = Emitter::new(plan);
    let mut labels = Labels::new("b");
    let mut solutions = 0;
    let mut rows = Vec::with_capacity(records.len());
    for (i, envs) in records.into_iter().enumerate() {
        labels.reset(format!("b{}x", first + i as u64));
        let mut triples = Vec::new();
        for mut env in envs {
            solutions += 1;
            emitter.emit(&mut env, &mut labels, &mut |t| triples.push(t));
        }
        rows.push(triples);
    }
    (solutions, rows)
}
