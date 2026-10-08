//! tarka over Polars data frames.
//!
//! A frame's columns are turned into cells, column by column, and the plan then runs
//! exactly as it does over CSV: the same lifting, the same shape executor, batched and
//! in parallel. So a frame gives the same RDF as a CSV file with the same text.
//!
//! How Polars values become cell text:
//!
//! | Polars type                         | cell                                         |
//! |-------------------------------------|----------------------------------------------|
//! | `String`, `Categorical`, `Enum`     | the string (empty or blank: unbound)         |
//! | integers, `Decimal`, `Boolean`      | the value (`42`, `9.95`, `true`)             |
//! | `Float32`, `Float64`                | the shortest form; `INF`, `-INF`, `NaN`      |
//! | `Date`                              | `2024-03-05`                                 |
//! | `Datetime`                          | `2024-03-05T10:15:00`, in UTC with `Z` when the column has a time zone |
//! | `Time`                              | `10:15:00`                                   |
//! | `Duration`                          | `PT90S`                                      |
//! | `List`                              | a list cell, for an OTTR list parameter (no separator needed) |
//! | null                                | unbound                                      |
//!
//! Only the columns a plan uses are converted.
//!
//! A plan whose lifting reads columns (OTTR templates) is lifted column by column, in
//! parallel: each column becomes its parameter's terms directly (an integer column
//! becomes `xsd:integer` literals without being written out and read back), and the
//! rows go straight to the shape layer. A TARQL query's rows are turned into cells and
//! evaluated as CSV rows are.

/// The Polars version tarka is built with.
pub use polars;

use std::convert::Infallible;
use std::path::Path;

use oxrdf::vocab::xsd;
use oxrdf::{Literal, NamedNode, NamedNodeRef, Triple};
use polars::prelude::*;
use rayon::prelude::*;
use tarka::{RunError, RunOptions, RunStats};
use tarka_core::{Cell, CellSource, ColumnBinding, Conversion, Lifting, Plan, Record, Value};
use tarka_io::TripleSink;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FrameError {
    #[error("{0}")]
    Polars(#[from] PolarsError),
    #[error("column {column}: {dtype} values cannot be mapped")]
    Unsupported { column: String, dtype: String },
    #[error("column {0} holds lists, and a TARQL query binds text: join them first (for example with Polars' list.join)")]
    ListForSparql(String),
    #[error("{0}")]
    Run(#[from] RunError),
}

#[derive(Clone, Debug, Default)]
pub struct FrameOptions {
    /// Bind empty and blank strings instead of leaving them unbound.
    pub bind_empty_strings: bool,
}

/// The names of the columns `plan` reads (those it does not read stay unconverted).
fn used_columns(plan: &Plan, columns: &[String]) -> Vec<String> {
    match &plan.lifting {
        Lifting::Columns(bindings) => bindings
            .iter()
            .filter_map(|b| match &b.source {
                CellSource::Column(c) => Some(c.clone()),
                CellSource::RowNumber => None,
            })
            .collect(),
        Lifting::Sparql(lifting) => columns.iter().filter(|c| lifting.variables.contains(c)).cloned().collect(),
        Lifting::Given => Vec::new(),
    }
}

/// The cells of a frame's columns: the column names (all of them) and one record per
/// row. Columns the plan does not read are left empty.
pub fn frame_records(plan: &Plan, df: &DataFrame, options: &FrameOptions) -> Result<(Vec<String>, Vec<Record>), FrameError> {
    let columns: Vec<String> = df.get_column_names().iter().map(|c| c.to_string()).collect();
    let used = used_columns(plan, &columns);
    let mut records: Vec<Record> = (0..df.height()).map(|i| Record { row: i as u64, cells: vec![None; columns.len()] }).collect();
    for (index, name) in columns.iter().enumerate() {
        if !used.contains(name) {
            continue;
        }
        let series = df.column(name)?.as_materialized_series().clone();
        if matches!(plan.lifting, Lifting::Sparql(_)) && matches!(series.dtype(), DataType::List(_)) {
            return Err(FrameError::ListForSparql(name.clone()));
        }
        for (record, cell) in records.iter_mut().zip(cells(name, &series, options)?) {
            record.cells[index] = cell;
        }
    }
    Ok((columns, records))
}

/// Runs `plan` over `df` and writes the triples to `sink`.
pub fn run_frame(
    plan: &Plan,
    df: &DataFrame,
    options: &FrameOptions,
    sink: &mut dyn TripleSink,
    run: &RunOptions,
) -> Result<RunStats, FrameError> {
    if let Lifting::Columns(bindings) = &plan.lifting {
        let columns: Vec<Vec<Option<Value>>> =
            bindings.par_iter().map(|b| lift_column(plan, b, df, options)).collect::<Result<_, FrameError>>()?;
        let vars: Vec<usize> = bindings.iter().map(|b| b.var.0).collect();
        let mut columns: Vec<_> = columns.into_iter().map(Vec::into_iter).collect();
        let width = plan.vars.len();
        let envs = (0..df.height()).map(move |_| {
            let mut env = vec![None; width];
            for (var, column) in vars.iter().zip(columns.iter_mut()) {
                env[*var] = column.next().flatten();
            }
            Ok::<_, RunError>(env)
        });
        return Ok(tarka::run_envs(plan, envs, sink, run)?);
    }
    let (columns, records) = frame_records(plan, df, options)?;
    Ok(tarka::run(plan, &columns, records.into_iter().map(Ok::<_, Infallible>), sink, run)?)
}

/// The datatypes whose canonical form for an integer is the integer as Rust writes it.
const INTEGER_FORMS: [NamedNodeRef<'static>; 14] = [
    xsd::INTEGER,
    xsd::DECIMAL,
    xsd::LONG,
    xsd::INT,
    xsd::SHORT,
    xsd::BYTE,
    xsd::NON_NEGATIVE_INTEGER,
    xsd::POSITIVE_INTEGER,
    xsd::NON_POSITIVE_INTEGER,
    xsd::NEGATIVE_INTEGER,
    xsd::UNSIGNED_LONG,
    xsd::UNSIGNED_INT,
    xsd::UNSIGNED_SHORT,
    xsd::UNSIGNED_BYTE,
];

/// One binding's values for every row of the frame, the same as the cells' would be.
fn lift_column(plan: &Plan, b: &ColumnBinding, df: &DataFrame, options: &FrameOptions) -> Result<Vec<Option<Value>>, FrameError> {
    let height = df.height();
    let name = match &b.source {
        CellSource::RowNumber => return Ok((0..height).map(|i| Some(Value::Term(Literal::from(i as i64).into()))).collect()),
        CellSource::Column(c) => c,
    };
    let Ok(column) = df.column(name) else { return Ok(vec![None; height]) };
    let s = column.as_materialized_series();
    let blank = |t: &str| !options.bind_empty_strings && t.trim().is_empty();
    let text = |t: Option<&str>| -> Result<Option<Value>, FrameError> {
        match t {
            Some(t) if !blank(t) => Ok(tarka::engine::convert_text(t, b, plan)?),
            _ => Ok(None),
        }
    };
    // the cases that need no text
    match (s.dtype(), &b.conversion, b.list) {
        (DataType::String, _, _) => return s.str()?.iter().map(text).collect(),
        (dt, Conversion::Typed(datatype), false) if dt.is_integer() && INTEGER_FORMS.contains(&datatype.as_ref()) => {
            let ints = s.cast(&DataType::Int64);
            if let Ok(ints) = ints
                && ints.null_count() == s.null_count()
            {
                return Ok(ints.i64()?.iter().map(|v| v.map(|v| integer(v, datatype))).collect());
            }
        }
        (DataType::Boolean, Conversion::Typed(datatype), false) if datatype.as_ref() == xsd::BOOLEAN => {
            return Ok(s.bool()?.iter().map(|v| v.map(|v| Value::Term(Literal::from(v).into()))).collect());
        }
        (DataType::List(_), _, _) => {
            let lists = s.list()?;
            let mut out = Vec::with_capacity(height);
            for i in 0..lists.len() {
                out.push(match lists.get_as_series(i) {
                    None => None,
                    Some(inner) => {
                        let items = texts(name, &inner)?;
                        tarka::engine::convert_items(items.iter().flatten().map(String::as_str), b, plan)?
                    }
                });
            }
            return Ok(out);
        }
        _ => {}
    }
    texts(name, s)?.iter().map(|t| text(t.as_deref())).collect()
}

fn integer(v: i64, datatype: &NamedNode) -> Value {
    Value::Term(Literal::new_typed_literal(v.to_string(), datatype.clone()).into())
}

/// Runs `plan` over `df` and returns the triples as a frame of N-Triples terms, with
/// the columns `subject`, `predicate` and `object`. Duplicates are removed.
pub fn triplify(plan: &Plan, df: &DataFrame, options: &FrameOptions) -> Result<DataFrame, FrameError> {
    let mut triples: Vec<Triple> = Vec::new();
    run_frame(plan, df, options, &mut triples, &RunOptions::default())?;
    // each triple's first occurrence, in order, without copying any
    let mut seen = PlHashSet::with_capacity(triples.len());
    let firsts: Vec<&Triple> = triples.iter().filter(|t| seen.insert(*t)).collect();
    drop(seen);
    frame_of(&firsts)
}

/// A frame of N-Triples terms (`subject`, `predicate`, `object`) for `triples`.
pub fn triples_frame(triples: &[Triple]) -> Result<DataFrame, FrameError> {
    frame_of(&triples.iter().collect::<Vec<_>>())
}

/// The frame of `triples`: each column built in parallel chunks, each term written into
/// one reused buffer rather than a string of its own.
fn frame_of(triples: &[&Triple]) -> Result<DataFrame, FrameError> {
    use std::fmt::Write;
    const CHUNK: usize = 1 << 16;
    fn column(name: &str, triples: &[&Triple], term: fn(&Triple, &mut String)) -> Result<Column, FrameError> {
        let chunks: Vec<StringChunked> = triples
            .par_chunks(CHUNK)
            .map(|chunk| {
                let mut builder = StringChunkedBuilder::new(name.into(), chunk.len());
                let mut buf = String::new();
                for t in chunk {
                    buf.clear();
                    term(t, &mut buf);
                    builder.append_value(&buf);
                }
                builder.finish()
            })
            .collect();
        let mut chunks = chunks.into_iter();
        let mut out = chunks.next().unwrap_or_else(|| StringChunkedBuilder::new(name.into(), 0).finish());
        for c in chunks {
            out.append(&c)?;
        }
        Ok(out.into_series().into_column())
    }
    let ((s, p), o) = rayon::join(
        || {
            rayon::join(
                || {
                    column("subject", triples, |t, b| {
                        let _ = write!(b, "{}", t.subject);
                    })
                },
                || {
                    column("predicate", triples, |t, b| {
                        let _ = write!(b, "{}", t.predicate);
                    })
                },
            )
        },
        || {
            column("object", triples, |t, b| {
                let _ = write!(b, "{}", t.object);
            })
        },
    );
    Ok(DataFrame::new(triples.len(), vec![s?, p?, o?])?)
}

/// Reads a data frame from a Parquet (`.parquet`) or Arrow IPC (`.arrow`, `.ipc`,
/// `.feather`) file.
pub fn read_frame(path: &Path) -> Result<DataFrame, FrameError> {
    let file = std::fs::File::open(path).map_err(|e| PolarsError::IO { error: e.into(), msg: None })?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    Ok(match ext.as_str() {
        "parquet" => ParquetReader::new(file).finish()?,
        "arrow" | "ipc" | "feather" => IpcReader::new(file).finish()?,
        _ => return Err(PolarsError::ComputeError(format!("{}: not a Parquet or Arrow IPC file", path.display()).into()).into()),
    })
}

/// Whether tarka reads `path` as a data frame (rather than as CSV).
pub fn is_frame_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref(),
        Some("parquet" | "arrow" | "ipc" | "feather")
    )
}

/// The cells of one column.
fn cells(name: &str, s: &Series, options: &FrameOptions) -> Result<Vec<Option<Cell>>, FrameError> {
    let text = |t: Option<String>| -> Option<Cell> { t.filter(|t| options.bind_empty_strings || !t.trim().is_empty()).map(Cell::Text) };
    if let DataType::List(_) = s.dtype() {
        let lists = s.list()?;
        let mut out = Vec::with_capacity(s.len());
        for i in 0..lists.len() {
            out.push(match lists.get_as_series(i) {
                None => None,
                Some(inner) => Some(Cell::List(texts(name, &inner)?.into_iter().flatten().collect())),
            });
        }
        return Ok(out);
    }
    Ok(texts(name, s)?.into_iter().map(text).collect())
}

/// The text of each value of a (non-list) column.
fn texts(name: &str, s: &Series) -> Result<Vec<Option<String>>, FrameError> {
    let unsupported = || FrameError::Unsupported { column: name.to_owned(), dtype: s.dtype().to_string() };
    Ok(match s.dtype() {
        DataType::String => s.str()?.iter().map(|v| v.map(str::to_owned)).collect(),
        DataType::Float64 => s.f64()?.iter().map(|v| v.map(float)).collect(),
        DataType::Float32 => s.f32()?.iter().map(|v| v.map(|f| float(f as f64))).collect(),
        DataType::Date => s.date()?.as_date_iter().map(|v| v.map(|d| d.format("%Y-%m-%d").to_string())).collect(),
        DataType::Datetime(_, tz) => {
            let zoned = tz.is_some();
            s.datetime()?
                .as_datetime_iter()
                .map(|v| v.map(|d| format!("{}{}", d.format("%Y-%m-%dT%H:%M:%S%.f"), if zoned { "Z" } else { "" })))
                .collect()
        }
        DataType::Time => s.time()?.as_time_iter().map(|v| v.map(|t| t.format("%H:%M:%S%.f").to_string())).collect(),
        DataType::Duration(_) => {
            let us = s.cast(&DataType::Duration(TimeUnit::Microseconds))?;
            us.duration()?.physical().iter().map(|v| v.map(duration)).collect()
        }
        DataType::Null => vec![None; s.len()],
        DataType::Binary | DataType::Struct(_) => return Err(unsupported()),
        _ => {
            // integers, booleans, decimals, categoricals: Polars' own text
            let cast = s.cast(&DataType::String).map_err(|_| unsupported())?;
            cast.str()?.iter().map(|v| v.map(str::to_owned)).collect()
        }
    })
}

/// A float in XSD's lexical space: the shortest form that reads back as the value.
fn float(f: f64) -> String {
    if f.is_nan() {
        "NaN".into()
    } else if f.is_infinite() {
        if f > 0.0 { "INF".into() } else { "-INF".into() }
    } else {
        format!("{f:?}")
    }
}

/// An `xsd:duration` for a number of microseconds.
fn duration(us: i64) -> String {
    let sign = if us < 0 { "-" } else { "" };
    let us = us.unsigned_abs();
    let (secs, frac) = (us / 1_000_000, us % 1_000_000);
    if frac == 0 { format!("{sign}PT{secs}S") } else { format!("{sign}PT{secs}.{}S", format!("{frac:06}").trim_end_matches('0')) }
}
