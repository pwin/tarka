//! Every mapping against its reference output.
//!
//! The references for TARQL are oxi-gen's output (`expected/*.nt`), with one
//! deliberate difference: `extra/bound.rq`, where tarka gives TARQL's answer for
//! `BOUND(?column)`. The references for OTTR are Lutra's.

mod common;

use common::*;
use oxrdf::Graph;
use tarka::io::CsvOptions;

/// (query, CSV, has a header row)
const TARQL_CASES: &[(&str, &str, bool)] = &[
    ("oxigen/escaped_chars.rq", "oxigen/escaped_chars.csv", true),
    ("oxigen/optional_field.rq", "oxigen/optional_field.csv", true),
    ("oxigen/quoted_empty.rq", "oxigen/quoted_empty.csv", true),
    ("oxigen/successor_field.rq", "oxigen/successor_field.csv", true),
    ("oxigen/with_dup.rq", "oxigen/data_100.csv", true),
    ("oxigen/splitfuncs.rq", "oxigen/split.csv", false),
    ("extra/people.rq", "extra/people.csv", true),
    ("extra/orgs.rq", "extra/orgs.csv", true),
    ("extra/constants.rq", "extra/constants.csv", true),
    ("extra/bound.rq", "extra/bound.csv", true),
];

fn expected_for(query: &str) -> Graph {
    let q = fixture(query);
    read_graph(&q.parent().unwrap().join("expected").join(format!("{}.nt", q.file_stem().unwrap().to_string_lossy())))
}

#[test]
fn tarql_mappings() {
    for (query, csv, has_headers) in TARQL_CASES {
        let actual = run_csv(&tarql(&fixture(query)), &fixture(csv), header(*has_headers));
        assert_same(&expected_for(query), &actual, query);
    }
}

const RETAIL: [(&str, &str); 3] = [("customers", "rt:CustomerRow"), ("products", "rt:ProductRow"), ("orders", "rt:OrderRow")];

/// The same transformation written as TARQL and as a modular OTTR library gives the
/// same RDF, term for term, as oxi-gen running the TARQL.
#[test]
fn retail_as_tarql_and_as_ottr() {
    for (dataset, root) in RETAIL {
        let csv = fixture(&format!("retail/{dataset}.csv"));
        let expected = read_graph(&fixture(&format!("retail/expected/{dataset}.nt")));
        let via_tarql = run_csv(&tarql(&fixture(&format!("retail/tarql/{dataset}.rq"))), &csv, CsvOptions::default());
        assert_same(&expected, &via_tarql, &format!("{dataset} (TARQL)"));
        let via_ottr = run_csv(&ottr(&[fixture("retail/ottr")], &[root], &[]), &csv, CsvOptions::default());
        assert_same(&expected, &via_ottr, &format!("{dataset} (OTTR)"));
    }
}

/// ottr2sparql's conversions run as they are: the library decomposed from the TARQL
/// (its root templates carry the WHERE clause in `tq:` annotations), and the library
/// from OTTR → TARQL → OTTR.
#[test]
fn converted_libraries() {
    for library in ["retail/converted/ottr-from-tarql", "retail/converted/ottr-round-trip"] {
        for (dataset, _) in RETAIL {
            let plan = ottr(&[fixture(library)], &[&format!("http://example.com/retail/q/{dataset}")], &[]);
            let actual = run_csv(&plan, &fixture(&format!("retail/{dataset}.csv")), CsvOptions::default());
            assert_same(&read_graph(&fixture(&format!("retail/expected/{dataset}.nt"))), &actual, &format!("{library} {dataset}"));
        }
    }
}

#[test]
fn ottr_libraries_match_lutra() {
    let products =
        run_csv(&ottr(&[fixture("extra/products.stottr")], &["t:Product"], &[]), &fixture("extra/products.csv"), CsvOptions::default());
    assert_same(&read_graph(&fixture("extra/expected/products_ottr.nt")), &products, "t:Product");
    let people = fixture("people/people.stottr");
    let person = run_csv(&ottr(std::slice::from_ref(&people), &["ex:Person"], &[]), &fixture("people/people.csv"), CsvOptions::default());
    assert_same(&read_graph(&fixture("people/expected/person.nt")), &person, "ex:Person");
    // a list column: one ex:HasSkill per skill, one record node per person
    let employee = run_csv(&ottr(&[people], &["ex:Employee"], &[("skills", ";")]), &fixture("people/people.csv"), CsvOptions::default());
    assert_same(&read_graph(&fixture("people/expected/employee.nt")), &employee, "ex:Employee");
}

#[test]
fn instances_expand_like_lutra() {
    for (library, instances, expected) in [
        ("extra/products.stottr", "extra/products.inst.stottr", "extra/expected/products_ottr.nt"),
        ("people/people.stottr", "people/person.inst.stottr", "people/expected/person.nt"),
        ("people/people.stottr", "people/employee.inst.stottr", "people/expected/employee.nt"),
    ] {
        let lib = tarka::ottr::Library::load(&[fixture(library)]).unwrap();
        let doc = tarka::ottr::parse_stottr(&std::fs::read_to_string(fixture(instances)).unwrap(), instances).unwrap();
        let mut triples = Vec::new();
        tarka::expand_instances(&lib, &doc.instances, &mut triples).unwrap();
        assert_same(&read_graph(&fixture(expected)), &triples.into_iter().collect(), instances);
    }
}
