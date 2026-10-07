//! Every bOTTR fixture against Lutra's output (`tests/fixtures/bottr/expected`, made by
//! Lutra), with the same instances left out. With `LUTRA_JAR` set, Lutra is run on them
//! too.

#[path = "../../tarka/tests/common/mod.rs"]
mod common;

use std::path::PathBuf;
use std::process::Command;

use common::*;
use oxrdf::{Graph, Triple};
use tarka::ottr::Library;

/// (bOTTR file, library, instances Lutra leaves out)
const CASES: &[(&str, &str, u64)] = &[
    ("bottr/people.bottr.ttl", "people/people.stottr", 0),
    ("bottr/args.bottr.ttl", "bottr/templates.stottr", 1),
    ("bottr/rdf.bottr.ttl", "bottr/templates.stottr", 0),
    ("bottr/sql.bottr.ttl", "bottr/templates.stottr", 0),
    ("bottr/lists.bottr.ttl", "bottr/templates.stottr", 0),
    ("bottr/h2.bottr.ttl", "bottr/templates.stottr", 4),
];

fn expected(map: &str) -> PathBuf {
    let name = map.trim_start_matches("bottr/").trim_end_matches(".bottr.ttl");
    fixture(&format!("bottr/expected/{name}.ttl"))
}

fn run(map: &str, library: &str) -> (Graph, u64, Vec<String>) {
    let lib = Library::load(&[fixture(library)]).unwrap();
    let maps = tarka_bottr::load(&fixture(map)).unwrap();
    let mut triples: Vec<Triple> = Vec::new();
    let mut reasons = Vec::new();
    let stats = tarka_bottr::run(&lib, &maps, &mut triples, &mut |r| reasons.push(r)).unwrap();
    (triples.into_iter().collect(), stats.dropped, reasons)
}

#[test]
fn maps_give_lutras_output() {
    for (map, library, left_out) in CASES {
        let (graph, dropped, reasons) = run(map, library);
        assert_same(&read_graph(&expected(map)), &graph, map);
        assert_eq!(dropped, *left_out, "{map}: {reasons:#?}");
    }
}

#[test]
fn reasons_name_the_map_row_and_argument() {
    let (_, _, reasons) = run("bottr/args.bottr.ttl", "bottr/templates.stottr");
    assert_eq!(reasons.len(), 1);
    let r = &reasons[0];
    assert!(r.contains("args.bottr.ttl: instance map 1 (ex:A), row 3: "), "{r}");
    assert!(r.contains("argument 5 (flag): the value 'maybe' is not in the lexical space of its datatype"), "{r}");
    assert!(r.contains("argument 6 (lang): value 'plain' does not contain language tag separator '@'"), "{r}");
    let (_, _, reasons) = run("bottr/h2.bottr.ttl", "bottr/templates.stottr");
    assert!(
        reasons[0].contains("instance map 2 (ex:P), row 1: argument 1 \"ex:a\" (xsd:string) does not fit ?id : ottr:IRI"),
        "{reasons:#?}"
    );
}

#[test]
fn maps_that_cannot_run() {
    let lib = Library::load(&[fixture("bottr/templates.stottr")]).unwrap();
    let dir = std::env::temp_dir().join(format!("tarka-bottr-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let attempt = |body: &str| {
        let path = dir.join("map.ttl");
        let text = format!(
            "@prefix ottr: <http://ns.ottr.xyz/0.4/> . @prefix ex: <http://example.com/ns#> . @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n{body}"
        );
        std::fs::write(&path, text).unwrap();
        let maps = tarka_bottr::load(&path)?;
        let mut triples: Vec<Triple> = Vec::new();
        tarka_bottr::run(&lib, &maps, &mut triples, &mut |_| {}).map(|_| ())
    };
    let csv = fixture("bottr/semi.csv").display().to_string().replace('\\', "/");
    let h2 = |template: &str, query: &str, maps: &str| {
        format!(
            "[] a ottr:InstanceMap ; ottr:template {template} ; ottr:source [ a ottr:H2Source ] ; ottr:query \"\"\"{query}\"\"\" {maps} ."
        )
    };
    let cases = [
        (h2("ex:Nope", &format!("SELECT * FROM CSVREAD('{csv}')"), ""), "not in the library"),
        (
            h2("ex:Q", &format!("SELECT * FROM CSVREAD('{csv}', 'A;B', 'fieldSeparator=;')"), ""),
            "the query gives 2 columns (A, B), but the template takes 4",
        ),
        (
            h2("ex:Q", &format!("SELECT * FROM CSVREAD('{csv}')"), "; ottr:argumentMaps ( [ ] )"),
            "1 argument maps, but the template takes 4",
        ),
        (h2("ex:Q", "SELECT a FROM people WHERE x = 1", ""), "tarka reads H2 queries of the form"),
        ("[] a ottr:InstanceMap ; ottr:template ex:Q ; ottr:source [ a ottr:JDBCSource ] ; ottr:query \"x\" .".into(), "is not one"),
        (
            h2("ex:Q", "x", "; ottr:argumentMaps ( [ ottr:type ottr:IRI ; ottr:languageTag \"en\" ] [ ] [ ] [ ] )"),
            "cannot have both a type and a language tag",
        ),
    ];
    for (body, message) in cases {
        let error = attempt(&body).unwrap_err().to_string();
        assert!(error.contains(message), "{body}\n→ {error}");
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn maps_match_lutra_live() {
    let Ok(lutra) = std::env::var("LUTRA_JAR") else {
        eprintln!("LUTRA_JAR is not set: not comparing with Lutra");
        return;
    };
    let dir = std::env::temp_dir().join(format!("tarka-bottr-lutra-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (map, library, _) in CASES {
        let out = dir.join("out.ttl");
        let run_lutra = Command::new("java")
            .args(["-jar", &lutra, "-m", "expand", "-I", "bottr", "-L", "stottr", "-O", "wottr"])
            .arg("-l")
            .arg(fixture(library))
            .arg("-o")
            .arg(&out)
            .arg(fixture(map))
            .output()
            .unwrap();
        assert!(out.exists(), "{map}: {}", String::from_utf8_lossy(&run_lutra.stderr));
        assert_same(&read_graph(&out), &run(map, library).0, map);
        std::fs::remove_file(&out).ok();
    }
    std::fs::remove_dir_all(&dir).ok();
}
