//! Helpers shared by the integration tests.
#![allow(dead_code)]

use std::fs::File;
use std::path::{Path, PathBuf};

use oxrdf::dataset::CanonicalizationAlgorithm;
use oxrdf::{Graph, Triple};
use oxrdfio::{RdfFormat, RdfParser};
use tarka::io::{CsvOptions, CsvSource};
use tarka::ottr::{CompileOptions, Library};
use tarka::{Plan, RunOptions};

pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

pub fn fixture(rel: &str) -> PathBuf {
    fixtures().join(rel)
}

/// An RDF file (N-Triples if it ends in `.nt`, else Turtle) as a graph.
pub fn read_graph(path: &Path) -> Graph {
    let format = if path.extension().is_some_and(|e| e == "nt") { RdfFormat::NTriples } else { RdfFormat::Turtle };
    let file = File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    RdfParser::from_format(format)
        .for_reader(file)
        .map(|q| {
            let q = q.unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            Triple::new(q.subject, q.predicate, q.object)
        })
        .collect()
}

fn canonical(g: &Graph) -> Graph {
    let mut g = g.clone();
    g.canonicalize(CanonicalizationAlgorithm::Unstable);
    g
}

fn lines(g: &Graph) -> Vec<String> {
    let mut out: Vec<String> = g.iter().map(|t| t.to_string()).collect();
    out.sort();
    out
}

/// Fails unless the graphs are isomorphic: the same terms, lexical forms included,
/// up to blank node labels.
pub fn assert_same(expected: &Graph, actual: &Graph, what: &str) {
    let (e, a) = (canonical(expected), canonical(actual));
    if e != a {
        let (el, al) = (lines(&e), lines(&a));
        let only_e: Vec<_> = el.iter().filter(|l| !al.contains(l)).collect();
        let only_a: Vec<_> = al.iter().filter(|l| !el.contains(l)).collect();
        panic!(
            "{what}: graphs differ ({} expected, {} actual triples)\nonly expected:\n{}\nonly actual:\n{}",
            expected.len(),
            actual.len(),
            only_e.iter().map(|l| format!("  {l}")).collect::<Vec<_>>().join("\n"),
            only_a.iter().map(|l| format!("  {l}")).collect::<Vec<_>>().join("\n"),
        );
    }
}

/// Runs a plan over a CSV file.
pub fn run_csv(plan: &Plan, csv: &Path, options: CsvOptions) -> Graph {
    let source = CsvSource::new(File::open(csv).unwrap(), options).unwrap();
    let columns = source.columns().to_vec();
    let mut triples: Vec<Triple> = Vec::new();
    tarka::run(plan, &columns, source, &mut triples, &RunOptions::default()).unwrap();
    triples.into_iter().collect()
}

pub fn tarql(query: &Path) -> Plan {
    let text = std::fs::read_to_string(query).unwrap();
    tarka::tarql::parse_tarql(&text, &query.file_stem().unwrap().to_string_lossy()).unwrap()
}

pub fn ottr(library: &[PathBuf], roots: &[&str], lists: &[(&str, &str)]) -> Plan {
    let lib = Library::load(library).unwrap();
    let options = CompileOptions { given: false, lists: lists.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect() };
    tarka::ottr::compile_many(&lib, roots, &options).unwrap()
}

pub fn header(has_headers: bool) -> CsvOptions {
    CsvOptions { has_headers, ..CsvOptions::default() }
}
