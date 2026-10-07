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

/// The Polars version tarka is built with.
pub use polars;

use std::convert::Infallible;
use std::path::Path;

use oxrdf::Triple;
use polars::prelude::*;
use tarka::{RunError, RunOptions, RunStats};
use tarka_core::{Cell, CellSource, Lifting, Plan, Record};
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
    let (columns, records) = frame_records(plan, df, options)?;
    Ok(tarka::run(plan, &columns, records.into_iter().map(Ok::<_, Infallible>), sink, run)?)
}

/// Runs `plan` over `df` and returns the triples as a frame of N-Triples terms, with
/// the columns `subject`, `predicate` and `object`. Duplicates are removed.
pub fn triplify(plan: &Plan, df: &DataFrame, options: &FrameOptions) -> Result<DataFrame, FrameError> {
    let mut triples: Vec<Triple> = Vec::new();
    run_frame(plan, df, options, &mut triples, &RunOptions::default())?;
    let mut seen = std::collections::HashSet::new();
    triples.retain(|t| seen.insert(t.clone()));
    triples_frame(&triples)
}

/// A frame of N-Triples terms (`subject`, `predicate`, `object`) for `triples`.
pub fn triples_frame(triples: &[Triple]) -> Result<DataFrame, FrameError> {
    let s: Vec<String> = triples.iter().map(|t| t.subject.to_string()).collect();
    let p: Vec<String> = triples.iter().map(|t| t.predicate.to_string()).collect();
    let o: Vec<String> = triples.iter().map(|t| t.object.to_string()).collect();
    Ok(DataFrame::new(
        triples.len(),
        vec![Column::new("subject".into(), s), Column::new("predicate".into(), p), Column::new("object".into(), o)],
    )?)
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
