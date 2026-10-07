//! tarka against the tools it replaces, run live.
//!
//! * `OXI_GEN=path/to/oxi_gen`: every TARQL mapping gives the same RDF as oxi-gen,
//!   except `extra/bound.rq`, where tarka follows TARQL (`BOUND(?column)` is true
//!   when the cell has a value; oxi-gen says false).
//! * `LUTRA_JAR=path/to/lutra.jar` (and `java` on the path): every instance file
//!   expands to the same RDF as with Lutra, and the linter finds what Lutra's does: the
//!   same kinds of finding in the same templates, as often (tarka's own checks aside),
//!   and nothing in the fixture libraries.
//!
//! Without the variables these tests pass without checking anything.

mod common;

use std::path::Path;
use std::process::Command;

use common::*;

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tarka-parity-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn tarql_mappings_match_oxigen() {
    let Ok(oxigen) = std::env::var("OXI_GEN") else {
        eprintln!("OXI_GEN is not set: not comparing with oxi-gen");
        return;
    };
    let cases = [
        ("oxigen/escaped_chars.rq", "oxigen/escaped_chars.csv", true),
        ("oxigen/optional_field.rq", "oxigen/optional_field.csv", true),
        ("oxigen/quoted_empty.rq", "oxigen/quoted_empty.csv", true),
        ("oxigen/successor_field.rq", "oxigen/successor_field.csv", true),
        ("oxigen/with_dup.rq", "oxigen/data_100.csv", true),
        ("oxigen/splitfuncs.rq", "oxigen/split.csv", false),
        ("extra/people.rq", "extra/people.csv", true),
        ("extra/orgs.rq", "extra/orgs.csv", true),
        ("extra/constants.rq", "extra/constants.csv", true),
        ("retail/tarql/customers.rq", "retail/customers.csv", true),
        ("retail/tarql/products.rq", "retail/products.csv", true),
        ("retail/tarql/orders.rq", "retail/orders.csv", true),
    ];
    for (query, csv, has_headers) in cases {
        let out = temp("oxigen.nt");
        let mut cmd = Command::new(&oxigen);
        cmd.args(["-q", path(&fixture(query)), "-i", path(&fixture(csv)), "-o", path(&out), "--ntriples"]);
        if !has_headers {
            cmd.arg("-H");
        }
        let status = cmd.output().unwrap();
        assert!(status.status.success(), "{query}: {}", String::from_utf8_lossy(&status.stderr));
        let actual = run_csv(&tarql(&fixture(query)), &fixture(csv), header(has_headers));
        assert_same(&read_graph(&out), &actual, query);
    }
}

#[test]
fn instances_match_lutra() {
    let Ok(lutra) = std::env::var("LUTRA_JAR") else {
        eprintln!("LUTRA_JAR is not set: not comparing with Lutra");
        return;
    };
    for (library, instances) in [
        ("extra/products.stottr", "extra/products.inst.stottr"),
        ("people/people.stottr", "people/person.inst.stottr"),
        ("people/people.stottr", "people/employee.inst.stottr"),
    ] {
        let out = temp("lutra.ttl");
        let run = Command::new("java")
            .args(["-jar", &lutra, "-m", "expand", "-l", path(&fixture(library)), "-L", "stottr", "-e", "stottr"])
            .args(["-I", "stottr", "-O", "wottr", "--haltOn", "ERROR", "-o", path(&out), path(&fixture(instances))])
            .output()
            .unwrap();
        assert!(run.status.success(), "{instances}: {}", String::from_utf8_lossy(&run.stderr));
        let lib = tarka::ottr::Library::load(&[fixture(library)]).unwrap();
        let doc = tarka::ottr::parse_stottr(&std::fs::read_to_string(fixture(instances)).unwrap(), instances).unwrap();
        let mut triples = Vec::new();
        tarka::expand_instances(&lib, &doc.instances, &mut triples).unwrap();
        assert_same(&read_graph(&out), &triples.into_iter().collect(), instances);
    }
}

/// Lutra's lint findings as (severity, check, template), with tarka's names for the checks.
fn lutra_lint(lutra: &str, library: &Path) -> Vec<(String, String, String)> {
    let run = Command::new("java").args(["-jar", lutra, "-m", "lint", "-L", "stottr", "-l", path(library)]).output().unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
    let rules = [
        ("[ERROR] Undefined template used in ", "error", "undefined-template"),
        ("[ERROR] Wrong number of arguments in instance ", "error", "arity"),
        ("[ERROR] Cyclic dependency in template ", "error", "cycle"),
        ("[WARNING] Unused parameter in template ", "warning", "unused-parameter"),
        ("[ERROR] Type error in template ", "error", "type"),
        ("[WARNING] There exist duplicate templates which may conflict with each other: ", "warning", "duplicate"),
    ];
    let mut out = Vec::new();
    for line in text.lines().filter(|l| l.starts_with('[')) {
        let Some((prefix, severity, check)) = rules.iter().find(|(p, ..)| line.starts_with(p)) else {
            panic!("a finding of Lutra's this test does not know: {line}");
        };
        let rest = &line[prefix.len()..];
        let (template, check) = match *check {
            // "… An instance of template T has 2 arguments …"
            "arity" => (rest.split("An instance of template ").nth(1).unwrap().split(' ').next().unwrap(), "arity"),
            "type" if rest.contains(": incompatible parameter types") => {
                (rest.split(": incompatible").next().unwrap(), "inconsistent-uses")
            }
            "type" => (rest.split(": incompatible").next().unwrap(), "type"),
            "duplicate" => (rest.trim(), "duplicate"),
            _ => (rest.split(". ").next().unwrap().trim_end_matches('.'), *check),
        };
        out.push((severity.to_string(), check.to_string(), template.to_owned()));
    }
    out.sort();
    out
}

#[test]
fn lint_matches_lutra() {
    let Ok(lutra) = std::env::var("LUTRA_JAR") else {
        eprintln!("LUTRA_JAR is not set: not comparing with Lutra");
        return;
    };
    use tarka::ottr::lint::{Check, Severity, lint};
    // tarka's own checks, which Lutra's linter does not make
    let own = [Check::UndeclaredVariable, Check::NonBlank, Check::Expander, Check::Triple, Check::UnknownType, Check::Default];
    for library in
        ["lint/flawed.stottr", "retail/ottr", "people/people.stottr", "extra/products.stottr", "retail/converted/ottr-round-trip"]
    {
        let lib = tarka::ottr::Library::load(&[fixture(library)]).unwrap();
        let mut ours: Vec<(String, String, String)> = lint(&lib)
            .into_iter()
            .filter(|f| !own.contains(&f.check))
            .map(|f| {
                let severity = if f.severity == Severity::Error { "error" } else { "warning" };
                (severity.to_owned(), f.check.name().to_owned(), f.template)
            })
            .collect();
        ours.sort();
        assert_eq!(ours, lutra_lint(&lutra, &fixture(library)), "{library}");
    }
}

fn path(p: &Path) -> &str {
    p.to_str().unwrap()
}
