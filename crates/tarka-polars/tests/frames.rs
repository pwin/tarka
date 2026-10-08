//! The data frame backend gives the same RDF as the CSV backend, and maps Polars'
//! own types (numbers, dates, lists).

#[path = "../../tarka/tests/common/mod.rs"]
mod common;

use std::fs::File;
use std::path::Path;

use common::*;
use oxrdf::{Graph, Triple};
use polars::prelude::*;
use tarka::RunOptions;
use tarka::io::{CsvOptions, CsvSource};
use tarka_core::Plan;
use tarka_polars::{FrameOptions, read_frame, run_frame, triplify};

/// A CSV file as a frame of string columns (null for an unbound cell), read with
/// tarka's CSV reader so that escapes and empty cells follow oxi-gen.
fn csv_frame(path: &Path, options: CsvOptions) -> DataFrame {
    let source = CsvSource::new(File::open(path).unwrap(), options).unwrap();
    let columns = source.columns().to_vec();
    let records: Vec<_> = source.map(Result::unwrap).collect();
    let cols: Vec<Column> = columns
        .iter()
        .enumerate()
        .filter(|(i, _)| records.iter().any(|r| r.cells.get(*i).is_some()))
        .map(|(i, name)| {
            let values: Vec<Option<String>> =
                records.iter().map(|r| r.cells.get(i).cloned().flatten().and_then(|c| c.text().map(str::to_owned))).collect();
            Column::new(name.as_str().into(), values)
        })
        .collect();
    DataFrame::new(records.len(), cols).unwrap()
}

fn run(plan: &Plan, df: &DataFrame) -> Graph {
    let mut triples: Vec<Triple> = Vec::new();
    run_frame(plan, df, &FrameOptions::default(), &mut triples, &RunOptions::default()).unwrap();
    triples.into_iter().collect()
}

#[test]
fn frames_give_the_csv_results() {
    let tarql_cases = [
        ("oxigen/optional_field.rq", "oxigen/optional_field.csv", true),
        ("oxigen/with_dup.rq", "oxigen/data_100.csv", true),
        ("oxigen/splitfuncs.rq", "oxigen/split.csv", false),
        ("extra/people.rq", "extra/people.csv", true),
        ("extra/orgs.rq", "extra/orgs.csv", true),
        ("extra/bound.rq", "extra/bound.csv", true),
    ];
    for (query, csv, has_headers) in tarql_cases {
        let df = csv_frame(&fixture(csv), header(has_headers));
        let q = fixture(query);
        let expected = read_graph(&q.parent().unwrap().join("expected").join(format!("{}.nt", q.file_stem().unwrap().to_string_lossy())));
        assert_same(&expected, &run(&tarql(&q), &df), query);
    }
    for (dataset, root) in [("customers", "rt:CustomerRow"), ("products", "rt:ProductRow"), ("orders", "rt:OrderRow")] {
        let df = csv_frame(&fixture(&format!("retail/{dataset}.csv")), CsvOptions::default());
        let expected = read_graph(&fixture(&format!("retail/expected/{dataset}.nt")));
        assert_same(&expected, &run(&tarql(&fixture(&format!("retail/tarql/{dataset}.rq"))), &df), dataset);
        assert_same(&expected, &run(&ottr(&[fixture("retail/ottr")], &[root], &[]), &df), dataset);
    }
}

/// A list column feeds an OTTR list parameter directly: no separator.
#[test]
fn list_columns_are_ottr_lists() {
    let df = df!(
        "person" => ["ex:alice", "ex:bob", "ex:carol"],
        "name" => ["Alice", "Bob", "Carol"],
        "email" => [Some("alice@example.com"), None, Some("carol@example.com")],
        "manager" => [Some("ex:carol"), Some("ex:carol"), None],
    )
    .unwrap();
    let skills = Series::new(
        "skills".into(),
        [Some(Series::new("".into(), ["python", "rust", "sparql"])), Some(Series::new("".into(), ["sparql"])), None],
    );
    let mut df = df;
    df.with_column(skills.into_column()).unwrap();
    let plan = ottr(&[fixture("people/people.stottr")], &["ex:Employee"], &[]);
    assert_same(&read_graph(&fixture("people/expected/employee.nt")), &run(&plan, &df), "ex:Employee from a list column");
}

/// Polars' typed columns become typed literals for typed OTTR parameters, in canonical
/// form, and plain text for TARQL.
#[test]
fn typed_columns() {
    use chrono::NaiveDate;
    let lib =
        "@prefix ex: <http://example.com/> . @prefix ottr: <http://ns.ottr.xyz/0.4/> . @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
         ex:Row [ ottr:IRI ?id, ? xsd:integer ?n, ? xsd:decimal ?d, ? xsd:double ?x, ? xsd:boolean ?b, ? xsd:date ?day,
                  ? xsd:dateTime ?at, ? xsd:dateTime ?utc, ? ?plain ] :: {
           ottr:Triple(?id, ex:n, ?n), ottr:Triple(?id, ex:d, ?d), ottr:Triple(?id, ex:x, ?x), ottr:Triple(?id, ex:b, ?b),
           ottr:Triple(?id, ex:day, ?day), ottr:Triple(?id, ex:at, ?at), ottr:Triple(?id, ex:utc, ?utc), ottr:Triple(?id, ex:plain, ?plain) } .";
    let mut library = tarka::ottr::Library::new();
    library.add(tarka::ottr::parse_stottr(lib, "inline").unwrap());
    let plan = tarka::ottr::compile(&library, "http://example.com/Row", &Default::default()).unwrap();
    let at = NaiveDate::from_ymd_opt(2024, 3, 5).unwrap().and_hms_milli_opt(10, 15, 0, 250).unwrap();
    let mut df = df!(
        "id" => ["ex:a"],
        "n" => [7i64],
        "d" => [9.5f64],
        "x" => [1.5e3f64],
        "b" => [true],
        "day" => [NaiveDate::from_ymd_opt(2024, 3, 5).unwrap()],
        "at" => [at],
        "plain" => [42i32],
    )
    .unwrap();
    let utc = Series::new("utc".into(), [at]).cast(&DataType::Datetime(TimeUnit::Milliseconds, Some(TimeZone::UTC))).unwrap();
    assert!(matches!(utc.dtype(), DataType::Datetime(_, Some(_))), "{:?}", utc.dtype());
    df.with_column(utc.into_column()).unwrap();
    let out = run(&plan, &df);
    let text = out.to_string();
    for expected in [
        "<http://example.com/n> \"7\"^^<http://www.w3.org/2001/XMLSchema#integer>",
        "<http://example.com/d> \"9.5\"^^<http://www.w3.org/2001/XMLSchema#decimal>",
        "<http://example.com/x> \"1500\"^^<http://www.w3.org/2001/XMLSchema#double>",
        "<http://example.com/b> \"true\"^^<http://www.w3.org/2001/XMLSchema#boolean>",
        "<http://example.com/day> \"2024-03-05\"^^<http://www.w3.org/2001/XMLSchema#date>",
        "<http://example.com/at> \"2024-03-05T10:15:00.25\"^^<http://www.w3.org/2001/XMLSchema#dateTime>",
        "<http://example.com/utc> \"2024-03-05T10:15:00.25Z\"^^<http://www.w3.org/2001/XMLSchema#dateTime>",
        "<http://example.com/plain> \"42\"",
    ] {
        assert!(text.contains(expected), "missing {expected} in\n{text}");
    }
}

/// The column-by-column lifting gives the terms the cells would: every fast path and
/// the edges around it (nulls, blanks, large and negative integers, floats as decimals).
#[test]
fn columns_lift_as_cells_would() {
    use chrono::NaiveDate;
    let lib =
        "@prefix ex: <http://example.com/> . @prefix ottr: <http://ns.ottr.xyz/0.4/> . @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
         ex:Row [ ottr:IRI ?id, ? xsd:integer ?i, ? xsd:int ?small, ? xsd:decimal ?d, ? xsd:decimal ?f, ? xsd:boolean ?b,
                  ? xsd:string ?s, ? xsd:integer ?big, ? xsd:date ?day, ? xsd:double ?x, ? ?ROWNUM ] :: {
           ottr:Triple(?id, ex:i, ?i), ottr:Triple(?id, ex:small, ?small), ottr:Triple(?id, ex:d, ?d), ottr:Triple(?id, ex:f, ?f),
           ottr:Triple(?id, ex:b, ?b), ottr:Triple(?id, ex:s, ?s), ottr:Triple(?id, ex:big, ?big), ottr:Triple(?id, ex:day, ?day),
           ottr:Triple(?id, ex:x, ?x), ottr:Triple(?id, ex:row, ?ROWNUM) } .";
    let mut library = tarka::ottr::Library::new();
    library.add(tarka::ottr::parse_stottr(lib, "inline").unwrap());
    let plan = tarka::ottr::compile(&library, "http://example.com/Row", &Default::default()).unwrap();
    let day = |d| NaiveDate::from_ymd_opt(2024, 3, d);
    let df = df!(
        "id" => ["ex:a", "ex:b", "ex:c", "not an iri"],
        "i" => [Some(7i64), Some(-3), None, Some(0)],
        "small" => [Some(1i16), None, Some(-32768), Some(5)],
        "d" => [Some(42i32), Some(0), Some(-1), None],
        "f" => [Some(9.5f64), Some(8990.0), Some(0.1), None],
        "b" => [Some(true), Some(false), None, Some(true)],
        "s" => [Some("text"), Some("  "), Some(""), None],
        "big" => [Some(u64::MAX), Some(1), None, Some(2)],
        "day" => [day(5), None, day(7), day(8)],
        "x" => [Some(1.5e3f64), Some(f64::NAN), None, Some(-0.0)],
    )
    .unwrap();
    // the cell path: cells first, then the row engine
    let (columns, records) = tarka_polars::frame_records(&plan, &df, &FrameOptions::default()).unwrap();
    let mut by_cells: Vec<Triple> = Vec::new();
    tarka::run(&plan, &columns, records.into_iter().map(Ok::<_, std::convert::Infallible>), &mut by_cells, &RunOptions::default()).unwrap();
    let by_cells: Graph = by_cells.into_iter().collect();
    assert_same(&by_cells, &run(&plan, &df), "columns against cells");
    assert!(by_cells.to_string().contains("\"18446744073709551615\"^^<http://www.w3.org/2001/XMLSchema#integer>"));
    let blanks = FrameOptions { bind_empty_strings: true };
    let (columns, records) = tarka_polars::frame_records(&plan, &df, &blanks).unwrap();
    let mut by_cells: Vec<Triple> = Vec::new();
    tarka::run(&plan, &columns, records.into_iter().map(Ok::<_, std::convert::Infallible>), &mut by_cells, &RunOptions::default()).unwrap();
    let mut by_columns: Vec<Triple> = Vec::new();
    run_frame(&plan, &df, &blanks, &mut by_columns, &RunOptions::default()).unwrap();
    assert_same(&by_cells.into_iter().collect(), &by_columns.into_iter().collect(), "with empty strings bound");
}

/// `triplify` gives each triple once, where it first appears, as N-Triples terms.
#[test]
fn triple_frames_keep_first_occurrences_in_order() {
    let df = csv_frame(&fixture("extra/people.csv"), CsvOptions::default());
    let plan = tarql(&fixture("extra/people.rq"));
    let mut triples: Vec<Triple> = Vec::new();
    run_frame(&plan, &df, &FrameOptions::default(), &mut triples, &RunOptions::default()).unwrap();
    let mut seen = std::collections::HashSet::new();
    let expected: Vec<[String; 3]> = triples
        .iter()
        .filter(|t| seen.insert(*t))
        .map(|t| [t.subject.to_string(), t.predicate.to_string(), t.object.to_string()])
        .collect();
    assert!(expected.len() < triples.len(), "the fixture repeats triples across rows");
    let frame = triplify(&plan, &df, &FrameOptions::default()).unwrap();
    let column = |name: &str| -> Vec<String> { frame.column(name).unwrap().str().unwrap().iter().map(|v| v.unwrap().to_owned()).collect() };
    let (s, p, o) = (column("subject"), column("predicate"), column("object"));
    let got: Vec<[String; 3]> = (0..frame.height()).map(|i| [s[i].clone(), p[i].clone(), o[i].clone()]).collect();
    assert_eq!(got, expected);
    // an empty run gives an empty frame with the three columns
    let empty = triplify(&plan, &df.head(Some(0)), &FrameOptions::default()).unwrap();
    assert_eq!((empty.height(), empty.width()), (0, 3));
}

#[test]
fn parquet_files_and_triple_frames() {
    let mut df = csv_frame(&fixture("retail/orders.csv"), CsvOptions::default());
    let dir = std::env::temp_dir().join(format!("tarka-polars-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("orders.parquet");
    ParquetWriter::new(File::create(&path).unwrap()).finish(&mut df).unwrap();
    let back = read_frame(&path).unwrap();
    let plan = ottr(&[fixture("retail/ottr")], &["rt:OrderRow"], &[]);
    assert_same(&read_graph(&fixture("retail/expected/orders.nt")), &run(&plan, &back), "orders from Parquet");
    let triples = triplify(&plan, &back, &FrameOptions::default()).unwrap();
    let names: Vec<String> = triples.get_column_names().iter().map(|c| c.to_string()).collect();
    assert_eq!(names, ["subject", "predicate", "object"]);
    assert_eq!(triples.height(), 111);
    std::fs::remove_dir_all(&dir).ok();
}

/// List cells go to list parameters only, and text cells reach a list parameter only
/// with a separator.
#[test]
fn list_and_text_cells_must_match_the_parameters() {
    let error = |plan: &Plan, df: &DataFrame| {
        let mut triples: Vec<Triple> = Vec::new();
        run_frame(plan, df, &FrameOptions::default(), &mut triples, &RunOptions::default()).unwrap_err().to_string()
    };
    let lists = df!("person" => ["ex:alice"], "name" => ["Alice"])
        .unwrap()
        .hstack(&[Series::new("skills".into(), [Some(Series::new("".into(), ["rust"]))]).into_column()])
        .unwrap();
    let texts = df!("person" => ["ex:alice"], "name" => ["Alice"], "skills" => ["rust;sparql"]).unwrap();
    let employee = ottr(&[fixture("people/people.stottr")], &["ex:Employee"], &[]);
    let message = error(&employee, &texts);
    assert!(message.contains("?skills has a list type") && message.contains("--list skills"), "{message}");
    let lib = "@prefix ex: <http://example.com/> . @prefix ottr: <http://ns.ottr.xyz/0.4/> .
               ex:Tagged [ ottr:IRI ?person, ?skills ] :: { ottr:Triple(?person, ex:skill, ?skills) } .";
    let mut library = tarka::ottr::Library::new();
    library.add(tarka::ottr::parse_stottr(lib, "inline").unwrap());
    let tagged = tarka::ottr::compile(&library, "http://example.com/Tagged", &Default::default()).unwrap();
    let message = error(&tagged, &lists);
    assert!(message.contains("column skills holds lists, but its parameter does not take a list"), "{message}");
    let query = tarka::tarql::parse_tarql("CONSTRUCT { ?person <http://example.com/skill> ?skills } WHERE { }", "q").unwrap();
    let message = error(&query, &lists);
    assert!(message.contains("column skills holds lists, and a TARQL query binds text"), "{message}");
}
