//! The `tarka` command, run as a process.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures").join(rel)
}

fn tarka(args: &[&str], stdin: Option<&[u8]>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tarka"))
        .args(args)
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = stdin {
        child.stdin.take().unwrap().write_all(input).unwrap();
    }
    child.wait_with_output().unwrap()
}

fn ok(args: &[&str], stdin: Option<&[u8]>) -> String {
    let out = tarka(args, stdin);
    assert!(out.status.success(), "tarka {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

fn path(p: &Path) -> &str {
    p.to_str().unwrap()
}

fn sorted_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text.lines().filter(|l| !l.is_empty()).map(str::to_owned).collect();
    lines.sort();
    lines
}

/// oxi-gen's own options work: `-q -i -o --ntriples`, `--escape_char`, `--quote_char`.
#[test]
fn oxigen_command_lines() {
    let dir = std::env::temp_dir().join(format!("tarka-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("out.nt");
    let q = fixture("oxigen/successor_field.rq");
    let csv = fixture("oxigen/successor_field.csv");
    ok(&["run", "-q", path(&q), "-i", path(&csv), "-o", path(&out), "--ntriples", "--escape_char", "\\", "--quote_char", "\""], None);
    let written = std::fs::read_to_string(&out).unwrap();
    assert_eq!(sorted_lines(&written).len(), 6, "{written}");
    // the same from standard input to standard output
    let piped = ok(&["run", "-q", path(&q), "--ntriples"], Some(&std::fs::read(&csv).unwrap()));
    assert_eq!(sorted_lines(&piped), sorted_lines(&written));
    // gzip
    let gz = dir.join("out.nt.gz");
    ok(&["run", "-q", path(&q), "-i", path(&csv), "-o", path(&gz), "--ntriples", "--gzip"], None);
    let mut unzipped = String::new();
    flate2_read(&gz, &mut unzipped);
    assert_eq!(sorted_lines(&unzipped), sorted_lines(&written));
    std::fs::remove_dir_all(&dir).ok();
}

fn flate2_read(path: &Path, out: &mut String) {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(&bytes[..2], &[0x1f, 0x8b], "a gzip file");
    flate2::read::GzDecoder::new(&bytes[..]).read_to_string(out).unwrap();
}

#[test]
fn splits_tests_and_turtle() {
    let q = fixture("oxigen/splitfuncs.rq");
    let csv = fixture("oxigen/split.csv");
    let all = ok(&["run", "-q", path(&q), "-i", path(&csv), "-H", "--ntriples"], None);
    let first = ok(&["run", "-q", path(&q), "-i", path(&csv), "-H", "--ntriples", "--test", "1"], None);
    assert!(sorted_lines(&first).len() < sorted_lines(&all).len());
    let csv = "id,tags\nex:a,x;y\n";
    let q = "PREFIX ex: <http://example.com/>\nCONSTRUCT { ?s ex:tag ?tag } WHERE { BIND(tarql:expandPrefixedName(?id) AS ?s) }";
    let dir = std::env::temp_dir().join(format!("tarka-split-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("q.rq"), q).unwrap();
    let turtle = ok(&["run", "-q", path(&dir.join("q.rq")), "--split", "tags", "tag", ";"], Some(csv.as_bytes()));
    assert!(turtle.contains("@prefix ex: <http://example.com/> ."), "{turtle}");
    assert!(turtle.contains("ex:a ex:tag \"x\" , \"y\" ."), "{turtle}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn ottr_templates_modules_and_roots() {
    let retail = fixture("retail/ottr");
    let csv = fixture("retail/orders.csv");
    let by_dir = ok(&["run", "-l", path(&retail), "-T", "rt:OrderRow", "-i", path(&csv), "--ntriples"], None);
    let mut args = vec!["run".to_owned()];
    for m in ["base", "domain", "rows"] {
        args.extend(["-l".into(), retail.join(format!("{m}.stottr")).display().to_string()]);
    }
    args.extend(["-T".into(), "rt:OrderRow".into(), "-i".into(), csv.display().to_string(), "--ntriples".into()]);
    let by_module = ok(&args.iter().map(String::as_str).collect::<Vec<_>>(), None);
    assert_eq!(sorted_lines(&by_dir).len(), sorted_lines(&by_module).len());
    // two roots per row, and a list column
    let people = fixture("people/people.stottr");
    let pcsv = fixture("people/people.csv");
    let two = ok(
        &["run", "-l", path(&people), "-T", "ex:Person", "-T", "ex:Employee", "--list", "skills", ";", "-i", path(&pcsv), "--ntriples"],
        None,
    );
    assert!(two.contains("<http://example.com/ns#skill> \"rust\""), "{two}");
}

#[test]
fn expand_and_nquads() {
    let out = ok(
        &[
            "expand",
            "-l",
            path(&fixture("people/people.stottr")),
            path(&fixture("people/employee.inst.stottr")),
            "--graph",
            "http://example.com/g",
        ],
        None,
    );
    assert!(out.lines().all(|l| l.is_empty() || l.ends_with("<http://example.com/g> .")), "{out}");
    assert_eq!(sorted_lines(&out).len(), 26);
}

#[test]
fn errors_are_reported_cleanly() {
    let bad_query = tarka(&["run", "-q", path(&fixture("retail/customers.csv"))], Some(b""));
    assert!(!bad_query.status.success());
    let stderr = String::from_utf8_lossy(&bad_query.stderr);
    assert!(stderr.starts_with("tarka: "), "{stderr}");
    let ragged = tarka(&["run", "-q", path(&fixture("oxigen/successor_field.rq"))], Some(b"id,pref_label\nex:a\n"));
    assert!(!ragged.status.success());
    assert!(String::from_utf8_lossy(&ragged.stderr).contains("row 0"), "{}", String::from_utf8_lossy(&ragged.stderr));
    let unknown = tarka(&["run", "-l", path(&fixture("people/people.stottr")), "-T", "ex:Nope"], Some(b"x\n"));
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("is not defined"));
    // a list parameter's text cells need a separator
    let list = tarka(
        &["run", "-l", path(&fixture("people/people.stottr")), "-T", "ex:Employee"],
        Some(b"person,name,skills\nex:a,A,rust;sparql\n"),
    );
    assert!(!list.status.success());
    assert!(String::from_utf8_lossy(&list.stderr).contains("--list skills"), "{}", String::from_utf8_lossy(&list.stderr));
}

#[test]
fn shapes_and_validation() {
    let people = fixture("extra/people.rq");
    let shapes = ok(&["shapes", "-q", path(&people)], None);
    assert!(shapes.contains("shape:PersonShape a sh:NodeShape ;\n    sh:targetClass foaf:Person ;"), "{shapes}");
    let based = ok(&["shapes", "-q", path(&people), "--base", "http://example.com/shapes/"], None);
    assert!(based.contains("@prefix shape: <http://example.com/shapes/> ."), "{based}");
    // the fixture's output conforms to the shapes made from its mapping
    ok(&["run", "-q", path(&people), "-i", path(&fixture("extra/people.csv")), "--validate"], None);

    let dir = std::env::temp_dir().join(format!("tarka-cli-shapes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let lib = dir.join("items.stottr");
    std::fs::write(
        &lib,
        "@prefix ex: <http://example.com/> . @prefix ottr: <http://ns.ottr.xyz/0.4/> . @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
         @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
         ex:Item [ ottr:IRI ?id, ? xsd:integer ?qty ] :: { ottr:Triple(?id, rdf:type, ex:Item), ottr:Triple(?id, ex:qty, ?qty) } .",
    )
    .unwrap();
    let extra = dir.join("extra.ttl");
    std::fs::write(
        &extra,
        "@prefix sh: <http://www.w3.org/ns/shacl#> . @prefix ex: <http://example.com/> .
         ex:ItemQty a sh:NodeShape ; sh:targetClass ex:Item ; sh:property [ sh:path ex:qty ; sh:minCount 1 ] .",
    )
    .unwrap();
    let (output, report) = (dir.join("items.nt"), dir.join("report.ttl"));
    let csv = b"id,qty\nex:a,7\nex:b,seven\nex:c,\n";
    let base = ["run", "-l", path(&lib), "-T", "ex:Item", "--ntriples", "-o", path(&output)];
    // the generated shapes catch the ill-typed quantity; the extra ones the missing one
    let generated = tarka(&[&base[..], &["--validate"]].concat(), Some(csv));
    assert_eq!(generated.status.code(), Some(3), "{}", String::from_utf8_lossy(&generated.stderr));
    let stderr = String::from_utf8_lossy(&generated.stderr);
    assert!(stderr.contains("ex:b ex:qty sh:DatatypeConstraintComponent: \"seven\"^^xsd:integer"), "{stderr}");
    assert!(stderr.contains("does not conform to the shapes (1 result)"), "{stderr}");
    assert_eq!(std::fs::read_to_string(&output).unwrap().lines().count(), 5, "the output is written all the same");
    let both = tarka(&[&base[..], &["--validate", "--shapes", path(&extra), "--report", path(&report)]].concat(), Some(csv));
    assert_eq!(both.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&both.stderr).contains("(2 results)"));
    let report_text = std::fs::read_to_string(&report).unwrap();
    assert!(report_text.contains("sh:conforms false"), "{report_text}");
    assert!(report_text.contains("sh:MinCountConstraintComponent"), "{report_text}");
    // --shapes alone uses only those shapes
    let only = tarka(&[&base[..], &["--shapes", path(&extra)]].concat(), Some(csv));
    assert!(String::from_utf8_lossy(&only.stderr).contains("ex:c ex:qty sh:MinCountConstraintComponent"));
    assert!(String::from_utf8_lossy(&only.stderr).contains("(1 result)"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn parquet_input() {
    use tarka_polars::polars::prelude::*;
    let mut df = df!(
        "person" => ["ex:alice", "ex:bob", "ex:carol"],
        "name" => ["Alice", "Bob", "Carol"],
        "email" => [Some("alice@example.com"), None, Some("carol@example.com")],
        "manager" => [Some("ex:carol"), Some("ex:carol"), None],
    )
    .unwrap();
    let dir = std::env::temp_dir().join(format!("tarka-parquet-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("people.parquet");
    ParquetWriter::new(std::fs::File::create(&file).unwrap()).finish(&mut df).unwrap();
    let out = ok(&["run", "-l", path(&fixture("people/people.stottr")), "-T", "ex:Person", "-i", path(&file), "--ntriples"], None);
    assert_eq!(sorted_lines(&out).len(), 16, "{out}");
    assert!(out.contains("<http://example.com/ns#alice> <http://example.com/ns#reportsTo> <http://example.com/ns#carol>"));
    let split =
        tarka(&["run", "-l", path(&fixture("people/people.stottr")), "-T", "ex:Person", "-i", path(&file), "--split", "a", "b", ";"], None);
    assert!(String::from_utf8_lossy(&split.stderr).contains("list column"));
    std::fs::remove_dir_all(&dir).ok();
}
