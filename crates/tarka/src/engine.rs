//! The row backend: records in, triples out, one record at a time.
//!
//! Records are read in batches. Each batch is lifted (column conversions natively,
//! a SPARQL WHERE clause with spareval) and shaped on its own thread, and the triples
//! are written in input order. Blank node labels come from a record's position in
//! the input, so the output does not depend on how the work was split.
//!
//! A record with a cell its parameter cannot take (text that is not an IRI, for an IRI
//! type) is left out, and reported with every such cell, as bOTTR leaves out an
//! instance; the run goes on. An empty cell is OTTR's `none`, and is not reported.

use holos_engine::oxrdf;
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
    /// Records left out, because a cell could not be converted.
    pub left_out: u64,
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
    /// A record that cannot be made, with why: [`run`] and [`run_envs`] leave it out,
    /// report it and go on. Anywhere else it is an error.
    #[error("{0}")]
    LeftOut(String),
}

/// A record's solutions (environments), or why it is left out.
type Lifted = Result<Vec<Vec<Option<Value>>>, String>;

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
                check_columns(bindings, columns)?;
                let mut out = Vec::new();
                for b in bindings {
                    let index = match &b.source {
                        CellSource::RowNumber => None,
                        CellSource::Column(c) => match columns.iter().position(|x| x == c) {
                            Some(i) => Some(i),
                            // an optional parameter's column that is not there is always empty
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

    /// The solutions (environments) of each record, or why it is left out.
    fn lift(&self, plan: &Plan, records: &[Record], first: u64) -> Result<Vec<Lifted>, RunError> {
        let empty = || vec![None; plan.vars.len()];
        match self {
            Self::Columns(bindings) => records
                .iter()
                .map(|r| {
                    let mut env = empty();
                    // every cell's problem, not just the first
                    let mut problems = Vec::new();
                    for (var, index, b) in bindings {
                        env[var.0] = match index {
                            None => Some(Value::Term(Literal::from(r.row as i64).into())),
                            Some(i) => match r.cells.get(*i).and_then(Option::as_ref) {
                                Some(cell) => match convert(cell, b, plan) {
                                    Err(RunError::LeftOut(problem)) => {
                                        problems.push(problem);
                                        None
                                    }
                                    value => value?,
                                },
                                None => None,
                            },
                        };
                    }
                    Ok(if problems.is_empty() { Ok(vec![env]) } else { Err(left_out(r.row, &problems)) })
                })
                .collect(),
            Self::Sparql(evaluator, outputs) => Ok(evaluator
                .evaluate(records, first)?
                .into_iter()
                .map(|solutions| {
                    Ok(solutions
                        .into_iter()
                        .map(|solution| {
                            let mut env = empty();
                            for (var, value) in outputs.iter().zip(solution) {
                                env[var.0] = value.map(Value::Term);
                            }
                            env
                        })
                        .collect())
                })
                .collect()),
        }
    }
}

/// Checks that the input has a column for each parameter the plan reads, before any
/// row is read. A mandatory parameter without a default and without a column would
/// leave out every instance, so the run is refused; an optional one is never given,
/// which is reported on standard error. Both name the nearest column, as a column
/// named `floor_area` for a parameter `?floorArea`.
pub fn check_columns(bindings: &[ColumnBinding], columns: &[String]) -> Result<(), RunError> {
    // the missing columns, each once: roots may read a column in different ways
    let mut absent: Vec<(&str, bool)> = Vec::new();
    for b in bindings {
        if let CellSource::Column(c) = &b.source
            && !columns.contains(c)
        {
            match absent.iter_mut().find(|(name, _)| name == c) {
                Some((_, required)) => *required |= b.required,
                None => absent.push((c, b.required)),
            }
        }
    }
    let missing = |required: bool| -> Vec<String> {
        absent
            .iter()
            .filter(|(_, r)| *r == required)
            .map(|(c, _)| match nearest(c, columns) {
                Some(near) => format!("{c} (is it {near}?)"),
                None => c.to_string(),
            })
            .collect()
    };
    let shown = || {
        const MAX: usize = 20;
        let mut list = columns.iter().take(MAX).cloned().collect::<Vec<_>>().join(", ");
        if columns.len() > MAX {
            list.push_str(&format!(" and {} more", columns.len() - MAX));
        }
        if list.is_empty() { "none".to_owned() } else { list }
    };
    let optional = missing(false);
    if !optional.is_empty() {
        eprintln!(
            "warning: the input has no column for the optional parameter{} {}, so {} never given (the columns: {})",
            if optional.len() == 1 { "" } else { "s" },
            optional.join(", "),
            if optional.len() == 1 { "it is" } else { "they are" },
            shown()
        );
    }
    let required = missing(true);
    if !required.is_empty() {
        return Err(RunError::Mapping(format!(
            "the input has no column for the mandatory parameter{} {}, so every instance would be left out (the columns: {}). \
             A parameter reads the column of its own name",
            if required.len() == 1 { "" } else { "s" },
            required.join(", "),
            shown()
        )));
    }
    Ok(())
}

/// The column whose name is nearest to `name`: the same letters and digits, ignoring
/// case and separators, or else a name an edit or two from it (one for a short name).
fn nearest<'a>(name: &str, columns: &'a [String]) -> Option<&'a str> {
    let fold = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<Vec<char>>();
    let wanted = fold(name);
    let allowed = match wanted.len() {
        0..=3 => 0,
        4..=7 => 1,
        _ => 2,
    };
    let (distance, best) = columns.iter().map(|c| (edits(&wanted, &fold(c)), c.as_str())).min_by_key(|(d, _)| *d)?;
    (distance <= allowed).then_some(best)
}

/// The edit (Levenshtein) distance of two names.
fn edits(a: &[char], b: &[char]) -> usize {
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (diagonal + usize::from(ca != cb)).min(row[j] + 1).min(above + 1);
            diagonal = above;
        }
    }
    row[b.len()]
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

/// Why a record is left out: `row N: column c: …`, for each problem.
pub fn left_out(row: u64, problems: &[String]) -> String {
    format!("row {}: {}", row + 1, problems.join("; "))
}

fn cell_value(text: &str, b: &ColumnBinding, plan: &Plan) -> Result<Value, RunError> {
    b.conversion.apply(text, &plan.prefixes).map(Value::Term).map_err(|e| RunError::LeftOut(format!("column {}: {e}", column_of(b))))
}

fn list_value<'a>(items: impl IntoIterator<Item = &'a str>, b: &ColumnBinding, plan: &Plan) -> Result<Value, RunError> {
    let items = items.into_iter().filter(|part| !part.trim().is_empty()).map(|part| cell_value(part, b, plan));
    Ok(Value::List(items.collect::<Result<_, _>>()?))
}

/// A text cell's value for a column binding (see [`convert`]). A cell its parameter
/// cannot take is [`RunError::LeftOut`].
pub fn convert_text(text: &str, b: &ColumnBinding, plan: &Plan) -> Result<Option<Value>, RunError> {
    if !b.list {
        return cell_value(text, b, plan).map(Some);
    }
    match &b.list_separator {
        Some(sep) => list_value(text.split(sep.as_str()), b, plan).map(Some),
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
    list_value(items, b, plan).map(Some)
}

/// Runs `plan` over `records` (whose cells follow `columns`) and writes the triples to
/// `sink`, one record's worth at a time. Each record left out is passed to `left_out`,
/// with its reason, in input order.
pub fn run<E>(
    plan: &Plan,
    columns: &[String],
    records: impl IntoIterator<Item = Result<Record, E>>,
    sink: &mut dyn TripleSink,
    options: &RunOptions,
    left_out: &mut dyn FnMut(String),
) -> Result<RunStats, RunError>
where
    E: std::error::Error + Send + Sync + 'static,
{
    let lifter = Lifter::new(plan, columns)?;
    let batch_size = if lifter.per_row() { 1 } else { options.batch_size };
    let records = records.into_iter().map(|r| r.map_err(|e| RunError::Input(Box::new(e))));
    drive(records, batch_size, options.jobs, sink, left_out, |batch, first| {
        let envs = lifter.lift(plan, &batch, first)?;
        Ok(emit(plan, envs, first))
    })
}

/// Runs `plan` over rows whose values are made already, one environment per row (a
/// value or `None` for each of the plan's variables, by `VarId`): the shape layer only,
/// for a backend that lifts by itself. A row given as [`RunError::LeftOut`] is left out
/// and passed to `left_out`, as [`run`] does.
pub fn run_envs<E>(
    plan: &Plan,
    envs: impl IntoIterator<Item = Result<Vec<Option<Value>>, E>>,
    sink: &mut dyn TripleSink,
    options: &RunOptions,
    left_out: &mut dyn FnMut(String),
) -> Result<RunStats, RunError>
where
    E: Into<RunError>,
{
    let envs = envs.into_iter().map(|e| match e.map_err(Into::into) {
        Ok(mut env) => {
            env.resize(plan.vars.len(), None);
            Ok(Ok(vec![env]))
        }
        Err(RunError::LeftOut(why)) => Ok(Err(why)),
        Err(e) => Err(e),
    });
    drive(envs, options.batch_size, options.jobs, sink, left_out, |batch, first| Ok(emit(plan, batch, first)))
}

/// Reads up to `jobs` batches at a time, shapes them in parallel, and writes them in order.
fn drive<T: Send>(
    mut items: impl Iterator<Item = Result<T, RunError>>,
    batch_size: usize,
    jobs: usize,
    sink: &mut dyn TripleSink,
    left_out: &mut dyn FnMut(String),
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
            let (solutions, rows, reasons) = result?;
            stats.solutions += solutions;
            stats.left_out += reasons.len() as u64;
            reasons.into_iter().for_each(&mut *left_out);
            for triples in rows {
                stats.triples += triples.len() as u64;
                sink.row(triples)?;
            }
        }
    }
    sink.finish()?;
    Ok(stats)
}

/// A shaped batch: the number of solutions, each record's triples, and the reason for
/// each record left out.
type Shaped = (u64, Vec<Vec<Triple>>, Vec<String>);

/// Shapes one batch: each record's solutions (environments) into its triples. A record
/// left out keeps its place, with no triples, so the labels of the others do not move.
fn emit(plan: &Plan, records: Vec<Lifted>, first: u64) -> Shaped {
    let mut emitter = Emitter::new(plan);
    let mut labels = Labels::new("b");
    let mut solutions = 0;
    let mut rows = Vec::with_capacity(records.len());
    let mut reasons = Vec::new();
    for (i, lifted) in records.into_iter().enumerate() {
        labels.reset(format!("b{}x", first + i as u64));
        let mut triples = Vec::new();
        match lifted {
            Ok(envs) => {
                for mut env in envs {
                    solutions += 1;
                    emitter.emit(&mut env, &mut labels, &mut |t| triples.push(t));
                }
            }
            Err(why) => reasons.push(why),
        }
        rows.push(triples);
    }
    (solutions, rows, reasons)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tarka_core::Conversion;

    fn binding(column: &str, required: bool) -> ColumnBinding {
        let source = CellSource::Column(column.into());
        ColumnBinding { var: VarId(0), source, conversion: Conversion::Plain, list: false, list_separator: None, required }
    }

    fn columns(names: &[&str]) -> Vec<String> {
        names.iter().map(|c| c.to_string()).collect()
    }

    #[test]
    fn nearest_column() {
        let cols = columns(&["shop_id", "floor_area", "opened", "isbn13", "name"]);
        assert_eq!(nearest("floorArea", &cols), Some("floor_area"));
        assert_eq!(nearest("ShopID", &cols), Some("shop_id"));
        assert_eq!(nearest("opend", &cols), Some("opened"), "one edit");
        assert_eq!(nearest("floor_areas", &cols), Some("floor_area"));
        assert_eq!(nearest("nam", &cols), None, "a short name must match exactly");
        assert_eq!(nearest("price", &cols), None);
        assert_eq!(nearest("price", &[]), None);
    }

    #[test]
    fn missing_columns() {
        let cols = columns(&["id", "floor_area"]);
        assert!(check_columns(&[binding("id", true), binding("notes", false)], &cols).is_ok(), "an optional column may be missing");
        let e = check_columns(&[binding("id", true), binding("floorArea", true), binding("x", true)], &cols).unwrap_err().to_string();
        assert!(e.contains("mandatory parameters floorArea (is it floor_area?), x"), "{e}");
        assert!(e.contains("the columns: id, floor_area"), "{e}");
        let many: Vec<String> = (0..25).map(|i| format!("c{i}")).collect();
        let e = check_columns(&[binding("x", true)], &many).unwrap_err().to_string();
        assert!(e.contains("c19 and 5 more"), "{e}");
        let row_number = ColumnBinding { source: CellSource::RowNumber, ..binding("", true) };
        assert!(check_columns(&[row_number], &[]).is_ok());
        // a column read in two ways is named once, as mandatory if either reading is
        let e = check_columns(&[binding("x", false), binding("x", true)], &cols).unwrap_err().to_string();
        assert!(e.contains("mandatory parameter x, so"), "{e}");
    }

    #[test]
    fn refused_before_any_row_is_read() {
        let mut plan = Plan::new("t", Lifting::Columns(vec![binding("label", true)]));
        plan.var("label");
        let records = std::iter::from_fn(|| -> Option<Result<Record, io::Error>> { panic!("a row was read") });
        let mut sink: Vec<Triple> = Vec::new();
        let e = run(&plan, &columns(&["Label"]), records, &mut sink, &RunOptions::default(), &mut |_| {}).unwrap_err();
        assert!(e.to_string().contains("label (is it Label?)"), "{e}");
    }
}
