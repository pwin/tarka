//! tarka against TARQL proper, on queries that use Jena's property functions
//! (`apf:strSplit` …) and functions (`afn:localname` …): the RDF must be what TARQL 1.3
//! (Jena 4.8.0) made, recorded in `tests/fixtures/jena/expected`. Typed literals are
//! compared by value, since tarka writes them in canonical form (Jena's `"2.0e0"` is `"2"`).
//!
//! With `TARQL_JAR=path/to/tarql-1.3-all.jar` (and `java` on the path), TARQL is run again
//! and must agree.

use holos_engine::{oxrdf, oxrdfio};
mod common;

use std::process::Command;

use common::*;
use oxrdf::{Graph, Term, Triple};
use oxrdfio::{RdfFormat, RdfParser};
use tarka::io::CsvOptions;
use tarka_core::literal::typed_literal;

/// Each query, and the CSV file it reads.
const CASES: [(&str, &str); 6] = [
    ("strsplit", "strsplit"),
    ("strsplit-regex", "strsplit"),
    ("steps", "steps"),
    ("bound", "bound"),
    ("iri", "iri"),
    ("functions", "functions"),
];

/// The graph with every typed literal in canonical form.
fn by_value(g: &Graph) -> Graph {
    g.iter()
        .map(|t| {
            let object = match t.object.into_owned() {
                Term::Literal(l) if l.language().is_none() => typed_literal(l.value(), l.datatype()).into(),
                other => other,
            };
            Triple::new(t.subject.into_owned(), t.predicate.into_owned(), object)
        })
        .collect()
}

fn tarka(query: &str, csv: &str) -> Graph {
    let plan = tarql(&fixture(&format!("jena/{query}.rq")));
    run_csv(&plan, &fixture(&format!("jena/{csv}.csv")), CsvOptions::default())
}

#[test]
fn jena_functions_give_what_tarql_gives() {
    for (query, csv) in CASES {
        let expected = read_graph(&fixture(&format!("jena/expected/{query}.nt")));
        assert!(!expected.is_empty(), "{query}: an expected output");
        assert_same(&by_value(&expected), &by_value(&tarka(query, csv)), query);
    }
}

#[test]
fn tarql_live() {
    let Ok(jar) = std::env::var("TARQL_JAR") else {
        eprintln!("TARQL_JAR is not set: not running TARQL");
        return;
    };
    for (query, csv) in CASES {
        let (q, c) = (fixture(&format!("jena/{query}.rq")), fixture(&format!("jena/{csv}.csv")));
        let run = Command::new("java").args(["-jar", &jar, "--ntriples"]).arg(&q).arg(&c).output().unwrap();
        assert!(run.status.success(), "{query}: {}", String::from_utf8_lossy(&run.stderr));
        // TARQL writes its warnings (an old namespace …) among the triples
        let text: String = String::from_utf8_lossy(&run.stdout)
            .lines()
            .filter(|l| l.starts_with('<') || l.starts_with("_:"))
            .map(|l| format!("{l}\n"))
            .collect();
        let jena: Graph = RdfParser::from_format(RdfFormat::NTriples)
            .for_reader(text.as_bytes())
            .map(|q| {
                let q = q.unwrap();
                Triple::new(q.subject, q.predicate, q.object)
            })
            .collect();
        assert_same(&by_value(&jena), &by_value(&tarka(query, csv)), &format!("{query} (TARQL)"));
    }
}
