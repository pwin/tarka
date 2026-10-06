//! tarka: turn tables into RDF with TARQL queries or OTTR templates.
//!
//! A mapping (a TARQL query, or an OTTR template from a library) compiles to a
//! [`Plan`]; [`run`] evaluates a plan over records and writes triples to a sink.
//!
//! ```no_run
//! use tarka::{RunOptions, io::{CsvOptions, CsvSource, OutputOptions, RdfWriter}};
//!
//! let plan = tarka::tarql::parse_tarql(&std::fs::read_to_string("people.rq")?, "people")?;
//! let source = CsvSource::new(std::fs::File::open("people.csv")?, CsvOptions::default())?;
//! let columns = source.columns().to_vec();
//! let mut out = RdfWriter::create(None, &plan.prefixes, OutputOptions::default())?;
//! tarka::run(&plan, &columns, source, &mut out, &RunOptions::default())?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod engine;

use std::collections::HashMap;

pub use engine::{RunError, RunOptions, RunStats, run};
pub use tarka_core as core;
pub use tarka_core::Plan;
pub use tarka_io as io;
pub use tarka_ottr as ottr;
pub use tarka_tarql as tarql;

use tarka_core::{Emitter, Labels};
use tarka_io::TripleSink;
use tarka_ottr::{CompileOptions, Instance, Library, OttrError};

#[derive(Debug, thiserror::Error)]
pub enum ExpandError {
    #[error("line {line}: {source}")]
    Instance { line: usize, source: OttrError },
    #[error("{0}")]
    Output(#[from] std::io::Error),
}

/// Expands OTTR instances (with ground arguments) to triples, as Lutra does.
///
/// Blank nodes made by templates are fresh for each instance; a blank node written
/// in the instances themselves is one node throughout.
pub fn expand_instances(lib: &Library, instances: &[Instance], sink: &mut dyn TripleSink) -> Result<RunStats, ExpandError> {
    let mut plans: HashMap<String, Plan> = HashMap::new();
    let mut labels = Labels::new("i");
    let mut stats = RunStats::default();
    let given = CompileOptions { given: true, ..CompileOptions::default() };
    for inst in instances {
        let fail = |source| ExpandError::Instance { line: inst.line, source };
        for inst in tarka_ottr::unroll_instance(inst).map_err(fail)? {
            let iri = lib.resolve(&inst.template);
            if !plans.contains_key(&iri) {
                plans.insert(iri.clone(), tarka_ottr::compile(lib, &iri, &given).map_err(fail)?);
            }
            let plan = &plans[&iri];
            let mut env = tarka_ottr::instance_values(&inst).map_err(fail)?;
            let expected = lib.get(&iri).map_or(0, |t| t.params.len());
            if env.len() != expected {
                return Err(fail(OttrError::Arity { template: iri, expected, given: inst.args.len() }));
            }
            // the root's parameters come first; the rest (list elements …) start unbound
            env.resize(plan.vars.len(), None);
            labels.reset(format!("i{}x", stats.records));
            let mut triples = Vec::new();
            Emitter::new(plan).emit(&mut env, &mut labels, &mut |t| triples.push(t));
            stats.records += 1;
            stats.solutions += 1;
            stats.triples += triples.len() as u64;
            sink.row(triples)?;
        }
    }
    sink.finish()?;
    Ok(stats)
}
