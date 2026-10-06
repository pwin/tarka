//! tarka against the tools it replaces, run live.
//!
//! * `OXI_GEN=path/to/oxi_gen`: every TARQL mapping gives the same RDF as oxi-gen,
//!   except `extra/bound.rq`, where tarka follows TARQL (`BOUND(?column)` is true
//!   when the cell has a value; oxi-gen says false).
//! * `LUTRA_JAR=path/to/lutra.jar` (and `java` on the path): every instance file
//!   expands to the same RDF as with Lutra.
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

fn path(p: &Path) -> &str {
    p.to_str().unwrap()
}
