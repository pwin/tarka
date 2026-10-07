//! bOTTR for tarka: instance maps, the mapping files of [bOTTR](https://spec.ottr.xyz/bOTTR/0.1/),
//! run over CSV files and RDF files.
//!
//! An `ottr:InstanceMap` names a template, a source and a query; each row the query
//! gives is an instance of the template, its values made into arguments by the map's
//! argument maps (types, language tags, null values, translation tables, lists, blank
//! nodes; see [`convert`]). tarka reads two kinds of source:
//!
//! * `ottr:H2Source` with a query over H2's `CSVREAD`
//!   (`SELECT a, b FROM CSVREAD('@@THIS_DIR@@/people.csv')`), in H2's CSV dialect;
//! * `ottr:RDFFileSource` with a SPARQL SELECT query over its `ottr:sourceURL` files.
//!
//! As in Lutra, an instance whose arguments cannot be made, or do not fit the template's
//! parameter types, is left out and reported, and the others are made.

pub mod convert;
pub mod h2;
pub mod map;
pub mod rdf;

use std::path::Path;

use tarka_core::{Emitter, Labels, Plan};
use tarka_io::TripleSink;
use tarka_ottr::{CompileOptions, Library, OttrError};
use thiserror::Error;

pub use map::{ArgumentMap, InstanceMap, Source, load, parse};

/// What a source gives: the column names, and each row's values (None: missing).
pub type Table<T> = (Vec<String>, Vec<Vec<Option<T>>>);

#[derive(Debug, Error)]
pub enum BottrError {
    #[error("{0}: {1}")]
    Io(String, std::io::Error),
    /// A bOTTR file that cannot be read as instance maps.
    #[error("{file}: {message}")]
    Map { file: String, message: String },
    /// An instance map that cannot be run.
    #[error("{file}: instance map {index} ({template}): {message}")]
    Run { file: String, index: usize, template: String, message: String },
    #[error("{0}")]
    Ottr(#[from] OttrError),
    #[error("{0}")]
    Output(#[from] std::io::Error),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Rows the queries gave.
    pub rows: u64,
    /// Instances made.
    pub instances: u64,
    /// Instances left out (each reported).
    pub dropped: u64,
    pub triples: u64,
}

/// Loads the instance maps of bOTTR files.
pub fn load_all(paths: &[impl AsRef<Path>]) -> Result<Vec<InstanceMap>, BottrError> {
    let mut out = Vec::new();
    for p in paths {
        out.extend(map::load(p.as_ref())?);
    }
    Ok(out)
}

/// The rows an instance map's query gives: column labels, and the values.
fn rows(m: &InstanceMap) -> Result<(Vec<String>, Vec<Vec<convert::Raw>>), String> {
    use convert::Raw;
    match &m.source {
        Source::H2 => {
            let dir = Path::new(&m.file).parent().map(Path::to_owned).unwrap_or_default();
            let dir = std::path::absolute(&dir).unwrap_or(dir);
            let q = h2::parse_query(&m.query, &dir)?;
            let (labels, rows) = h2::rows(&q)?;
            let rows = rows.into_iter().map(|r| r.into_iter().map(|v| v.map_or(Raw::Null, Raw::Text)).collect()).collect();
            Ok((labels, rows))
        }
        Source::RdfFiles(files) => {
            let (labels, rows) = rdf::rows(files, &m.query)?;
            let rows = rows.into_iter().map(|r| r.into_iter().map(|v| v.map_or(Raw::Null, Raw::Term)).collect()).collect();
            Ok((labels, rows))
        }
        Source::Unsupported(class) => Err(format!(
            "tarka reads H2 sources over CSVREAD and RDF file sources; {class} is not one (export the data to CSV, or query it into a file)"
        )),
    }
}

/// Runs instance maps with the templates of `lib`, writing the triples to `sink`.
/// Each instance left out is passed to `dropped`, with its reason.
pub fn run(lib: &Library, maps: &[InstanceMap], sink: &mut dyn TripleSink, dropped: &mut dyn FnMut(String)) -> Result<Stats, BottrError> {
    let mut stats = Stats::default();
    let mut converter = convert::Converter::default();
    let mut labels = Labels::new("b");
    let given = CompileOptions { given: true, ..CompileOptions::default() };
    // every map is checked before any runs
    let mut plans: Vec<Plan> = Vec::new();
    for m in maps {
        let fail = |message: String| BottrError::Run { file: m.file.clone(), index: m.index, template: name(&m.template, lib), message };
        let Some(template) = lib.get(&m.template) else { return Err(fail("the template is not in the library".into())) };
        if let Some(args) = &m.arguments
            && args.len() != template.params.len()
        {
            return Err(fail(format!("it has {} argument maps, but the template takes {} arguments", args.len(), template.params.len())));
        }
        plans.push(tarka_ottr::compile(lib, &m.template, &given).map_err(|e| fail(e.to_string()))?);
    }
    for (m, plan) in maps.iter().zip(&plans) {
        let template = lib.get(&m.template).expect("checked above");
        let fail = |message: String| BottrError::Run { file: m.file.clone(), index: m.index, template: name(&m.template, lib), message };
        let (columns, rows) = rows(m).map_err(fail)?;
        if columns.len() != template.params.len() {
            return Err(fail(format!(
                "the query gives {} columns ({}), but the template takes {} arguments",
                columns.len(),
                columns.join(", "),
                template.params.len()
            )));
        }
        let defaults = vec![ArgumentMap::default(); columns.len()];
        let arg_maps = m.arguments.as_deref().unwrap_or(&defaults);
        for (r, row) in rows.iter().enumerate() {
            stats.rows += 1;
            // every argument's error, not just the first
            let mut args = Vec::with_capacity(row.len());
            let mut errors = Vec::new();
            for (i, (raw, a)) in row.iter().zip(arg_maps).enumerate() {
                match converter.argument(raw, a, &m.prefixes) {
                    Ok(v) => args.push(v),
                    Err(e) => errors.push(format!("argument {} ({}): {e}", i + 1, columns[i])),
                }
            }
            let made = if errors.is_empty() { Ok(args) } else { Err(errors.join("; ")) };
            let mut env = match made.and_then(|args| convert::check(&args, &template.params, &lib.prefixes).map(|()| args)) {
                Ok(args) => args,
                Err(e) => {
                    stats.dropped += 1;
                    dropped(format!("{}: instance map {} ({}), row {}: {e}", m.file, m.index, name(&m.template, lib), r + 1));
                    continue;
                }
            };
            env.resize(plan.vars.len(), None);
            labels.reset(format!("m{}r{}x", m.index, r + 1));
            let mut triples = Vec::new();
            Emitter::new(plan).emit(&mut env, &mut labels, &mut |t| triples.push(t));
            stats.instances += 1;
            stats.triples += triples.len() as u64;
            sink.row(triples)?;
        }
    }
    sink.finish()?;
    Ok(stats)
}

fn name(iri: &str, lib: &Library) -> String {
    lib.prefixes.compact(iri).unwrap_or_else(|| format!("<{iri}>"))
}
