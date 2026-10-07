//! The shapes made from a mapping describe what it makes: they are well formed, every
//! fixture's output conforms to its own shapes, and they catch values that do not.

#[path = "../../tarka/tests/common/mod.rs"]
mod common;

use std::path::PathBuf;

use common::*;
use oxrdf::{Graph, Triple};
use oxrdfio::{RdfFormat, RdfParser};
use tarka::Plan;
use tarka::io::CsvOptions;
use tarka_shacl::{NodeKind, NodeShape, Shapes, Target, shapes};

/// Every fixture mapping: (what, plan, CSV, CSV options).
fn cases() -> Vec<(String, Plan, PathBuf, CsvOptions)> {
    let mut out = Vec::new();
    for (query, csv, has_headers) in [
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
    ] {
        out.push((query.to_owned(), tarql(&fixture(query)), fixture(csv), header(has_headers)));
    }
    for (dataset, root) in [("customers", "rt:CustomerRow"), ("products", "rt:ProductRow"), ("orders", "rt:OrderRow")] {
        let csv = fixture(&format!("retail/{dataset}.csv"));
        let query = fixture(&format!("retail/tarql/{dataset}.rq"));
        out.push((format!("{dataset} (TARQL)"), tarql(&query), csv.clone(), CsvOptions::default()));
        out.push((format!("{dataset} (OTTR)"), ottr(&[fixture("retail/ottr")], &[root], &[]), csv.clone(), CsvOptions::default()));
        let converted = ottr(&[fixture("retail/converted/ottr-from-tarql")], &[&format!("http://example.com/retail/q/{dataset}")], &[]);
        out.push((format!("{dataset} (tq: lifting)"), converted, csv, CsvOptions::default()));
    }
    let people = fixture("people/people.stottr");
    out.push((
        "ex:Person".into(),
        ottr(std::slice::from_ref(&people), &["ex:Person"], &[]),
        fixture("people/people.csv"),
        CsvOptions::default(),
    ));
    out.push((
        "ex:Employee".into(),
        ottr(std::slice::from_ref(&people), &["ex:Employee"], &[("skills", ";")]),
        fixture("people/people.csv"),
        CsvOptions::default(),
    ));
    out.push((
        "t:Product".into(),
        ottr(&[fixture("extra/products.stottr")], &["t:Product"], &[]),
        fixture("extra/products.csv"),
        CsvOptions::default(),
    ));
    out
}

fn turtle(text: &str) -> Graph {
    RdfParser::from_format(RdfFormat::Turtle)
        .for_slice(text.as_bytes())
        .map(|q| {
            let q = q.unwrap_or_else(|e| panic!("{e}\n{text}"));
            Triple::new(q.subject, q.predicate, q.object)
        })
        .collect()
}

#[test]
fn the_turtle_is_the_shapes_graph() {
    for (what, plan, _, _) in cases() {
        let s = shapes(&plan, &Default::default());
        assert_same(&s.triples().into_iter().collect(), &turtle(&s.to_turtle()), &what);
    }
}

fn shape<'a>(s: &'a Shapes, target: &str) -> &'a NodeShape {
    s.shapes
        .iter()
        .find(|n| n.targets.iter().any(|t| matches!(t, Target::Class(c) | Target::Node(c) if c.as_str() == target)))
        .unwrap_or_else(|| panic!("no shape for {target} in\n{}", s.to_turtle()))
}

#[test]
fn shapes_of_a_tarql_mapping() {
    let s = shapes(&tarql(&fixture("extra/people.rq")), &Default::default());
    let text = s.to_turtle();
    let person = shape(&s, "http://xmlns.com/foaf/0.1/Person");
    assert_eq!(person.iri.as_str(), "urn:tarka:shapes:PersonShape");
    let property = |n: &NodeShape, path: &str| n.properties.iter().find(|p| p.path.as_str() == path).cloned().unwrap();
    let xsd = |l: &str| format!("http://www.w3.org/2001/XMLSchema#{l}");
    let name = property(person, "http://xmlns.com/foaf/0.1/name");
    assert!(!name.required, "a column can be empty\n{text}");
    assert_eq!(name.datatype.unwrap().as_str(), xsd("string"));
    let age = property(person, "http://xmlns.com/foaf/0.1/age");
    assert_eq!(age.datatype.unwrap().as_str(), xsd("integer"), "BIND(xsd:integer(?age) AS ?age_int)");
    let address = property(person, "https://schema.org/address");
    assert!(address.required, "a blank node is always there\n{text}");
    assert_eq!(address.node_kind, Some(NodeKind::BlankNode));
    assert_eq!(address.classes.iter().map(|c| c.as_str()).collect::<Vec<_>>(), ["https://schema.org/PostalAddress"]);
    let works_for = property(person, "http://example.com/ns#worksFor");
    assert_eq!(works_for.node_kind, Some(NodeKind::Iri));
    assert_eq!(works_for.classes.iter().map(|c| c.as_str()).collect::<Vec<_>>(), ["http://xmlns.com/foaf/0.1/Organization"]);
    let street = property(shape(&s, "https://schema.org/PostalAddress"), "https://schema.org/streetAddress");
    assert_eq!(street.datatype.unwrap().as_str(), xsd("string"));
    let dataset = shape(&s, "http://example.com/ns#dataset");
    let last_row = property(dataset, "http://example.com/ns#lastRow");
    assert!(last_row.required, "the row number is always bound\n{text}");
    assert!(text.contains("shape:PersonShape a sh:NodeShape ;\n    sh:targetClass foaf:Person ;"), "{text}");
}

#[cfg(feature = "validate")]
mod validation {
    use super::*;
    use tarka::RunOptions;
    use tarka::io::CsvSource;

    fn output(plan: &Plan, csv: &std::path::Path, options: CsvOptions) -> Vec<Triple> {
        let source = CsvSource::new(std::fs::File::open(csv).unwrap(), options).unwrap();
        let columns = source.columns().to_vec();
        let mut triples: Vec<Triple> = Vec::new();
        tarka::run(plan, &columns, source, &mut triples, &RunOptions::default()).unwrap();
        triples
    }

    #[test]
    fn every_fixture_conforms_to_its_shapes() {
        for (what, plan, csv, options) in cases() {
            let s = shapes(&plan, &Default::default());
            let data = output(&plan, &csv, options);
            let v = tarka_shacl::validate(&data, &s.triples()).unwrap();
            assert!(v.conforms, "{what}: {:#?}\n{}", v.findings, s.to_turtle());
        }
    }

    #[test]
    fn shapes_catch_bad_and_missing_values() {
        let lib = "@prefix ex: <http://example.com/> . @prefix ottr: <http://ns.ottr.xyz/0.4/> .
            @prefix xsd: <http://www.w3.org/2001/XMLSchema#> . @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
            ex:Item [ ottr:IRI ?id, xsd:integer ?qty, ? xsd:string ?note ] :: {
              ottr:Triple(?id, rdf:type, ex:Item), ottr:Triple(?id, ex:qty, ?qty), ottr:Triple(?id, ex:note, ?note) } .";
        let mut library = tarka::ottr::Library::new();
        library.add(tarka::ottr::parse_stottr(lib, "inline").unwrap());
        let plan = tarka::ottr::compile(&library, "http://example.com/Item", &Default::default()).unwrap();
        let s = shapes(&plan, &Default::default());
        let item = shape(&s, "http://example.com/Item");
        assert!(item.properties.iter().any(|p| p.path.as_str() == "http://example.com/qty" && p.required), "{}", s.to_turtle());
        let dir = std::env::temp_dir().join(format!("tarka-shacl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let csv = dir.join("items.csv");
        std::fs::write(&csv, "id,qty,note\nex:a,7,ok\nex:b,seven,\n").unwrap();
        let mut data = output(&plan, &csv, CsvOptions::default());
        std::fs::remove_dir_all(&dir).ok();
        // "seven" is kept as an ill-typed literal, which sh:datatype rejects
        let v = tarka_shacl::validate(&data, &s.triples()).unwrap();
        assert!(!v.conforms);
        assert_eq!(v.findings.len(), 1, "{:#?}", v.findings);
        assert_eq!(v.findings[0].focus_node, "<http://example.com/b>");
        assert_eq!(v.findings[0].component, "<http://www.w3.org/ns/shacl#DatatypeConstraintComponent>");
        assert_eq!(v.findings[0].value.as_deref(), Some("\"seven\"^^<http://www.w3.org/2001/XMLSchema#integer>"));
        assert!(!v.report.is_empty());
        // and a mandatory value that is not there
        data.retain(|t| !(t.subject.to_string() == "<http://example.com/a>" && t.predicate.as_str() == "http://example.com/qty"));
        let v = tarka_shacl::validate(&data, &s.triples()).unwrap();
        let components: Vec<&str> = v.findings.iter().map(|f| f.component.as_str()).collect();
        assert!(components.contains(&"<http://www.w3.org/ns/shacl#MinCountConstraintComponent>"), "{components:?}");
    }
}
