//! OTTR templates used as modules: libraries split across files, any template as the
//! root, several roots per row, signatures defined elsewhere, and one's own templates
//! built on a published library.

mod common;

use common::*;
use oxrdf::{Graph, Triple};
use oxrdfio::{RdfFormat, RdfParser};
use tarka::io::{CsvOptions, CsvSource};
use tarka::ottr::{CompileOptions, Library, compile_many, parse_stottr};
use tarka::{Plan, RunOptions};

fn turtle(text: &str) -> Graph {
    RdfParser::from_format(RdfFormat::Turtle)
        .for_slice(text)
        .map(|q| {
            let q = q.unwrap();
            Triple::new(q.subject, q.predicate, q.object)
        })
        .collect()
}

fn run_text(plan: &Plan, csv: &str) -> Graph {
    let source = CsvSource::new(csv.as_bytes(), CsvOptions::default()).unwrap();
    let columns = source.columns().to_vec();
    let mut triples: Vec<Triple> = Vec::new();
    tarka::run(plan, &columns, source, &mut triples, &RunOptions::default()).unwrap();
    triples.into_iter().collect()
}

fn compile(lib: &Library, roots: &[&str]) -> Plan {
    compile_many(lib, roots, &CompileOptions::default()).unwrap()
}

const PREFIXES: &str = "@prefix ex: <http://example.com/ns#> . @prefix schema: <https://schema.org/> .
    @prefix cur: <http://example.com/currency/> . @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
    @prefix ottr: <http://ns.ottr.xyz/0.4/> . @prefix rt: <http://example.com/retail/template#> .";

/// The retail library is three modules (generic, domain, rows); listing them one by
/// one, in any order, is the same as loading their directory.
#[test]
fn a_library_from_separate_modules() {
    let dir = fixture("retail/ottr");
    let modules = ["base", "domain", "rows"].map(|m| dir.join(format!("{m}.stottr")));
    let mut reversed = modules.clone();
    reversed.reverse();
    let csv = fixture("retail/orders.csv");
    let expected = read_graph(&fixture("retail/expected/orders.nt"));
    for library in [vec![dir.clone()], modules.to_vec(), reversed.to_vec()] {
        let plan = compile(&Library::load(&library).unwrap(), &["rt:OrderRow"]);
        assert_same(&expected, &run_csv(&plan, &csv, CsvOptions::default()), &format!("{library:?}"));
    }
}

/// A template from the middle of the library is a root of its own: its parameters are
/// read from columns of the same names, with its default and its mandatory value.
#[test]
fn any_template_is_a_root() {
    let lib = Library::load(&[fixture("retail/ottr")]).unwrap();
    let plan = compile(&lib, &["rt:Price"]);
    let csv = "thing,property,amount,currency\n\
               ex:p1,schema:priceSpecification,9.50,cur:NOK\n\
               ex:p2,ex:unitPrice,12,\n\
               ex:p3,ex:unitPrice,,cur:USD\n";
    // ex:p2 has no currency: the default; ex:p3 has no amount: no price node at all
    let expected = turtle(&format!(
        "{PREFIXES}
        ex:p1 schema:priceSpecification [ a schema:PriceSpecification ; schema:price 9.5 ; schema:priceCurrency cur:NOK ] .
        ex:p2 ex:unitPrice [ a schema:PriceSpecification ; schema:price \"12\"^^xsd:decimal ; schema:priceCurrency cur:EUR ] ."
    ));
    assert_same(&expected, &run_text(&plan, csv), "rt:Price as the root");
}

/// Several roots per row: a mapping put together from existing templates.
#[test]
fn several_roots_per_row() {
    let lib = Library::load(&[fixture("people/people.stottr")]).unwrap();
    let plan = compile(&lib, &["ex:Person", "ex:HasSkill"]);
    let csv = "person,name,email,manager,skill\nex:alice,Alice,,ex:carol,rust\nex:bob,Bob,b@example.com,,\n";
    let expected = turtle(&format!(
        "{PREFIXES}
        ex:alice a schema:Person ; schema:name \"Alice\" ; ex:reportsTo ex:carol ;
            ex:reporting [ ex:manager ex:carol ; ex:source \"hr.csv\" ] ; ex:skill \"rust\" .
        ex:bob a schema:Person ; schema:name \"Bob\" ; schema:email \"b@example.com\" ."
    ));
    assert_same(&expected, &run_text(&plan, csv), "ex:Person + ex:HasSkill");
}

/// Roots that read one column with different types each get their own value.
#[test]
fn roots_read_a_column_by_their_own_types() {
    let mut lib = Library::new();
    lib.add(
        parse_stottr(
            &format!(
                "{PREFIXES}
                ex:AsNumber [ xsd:integer ?n ] :: {{ ottr:Triple(ex:s, ex:number, ?n) }} .
                ex:AsText [ ?n ] :: {{ ottr:Triple(ex:s, ex:text, ?n) }} ."
            ),
            "inline",
        )
        .unwrap(),
    );
    let plan = compile(&lib, &["ex:AsNumber", "ex:AsText"]);
    let expected = turtle(&format!("{PREFIXES} ex:s ex:number 7 ; ex:text \"007\" ."));
    assert_same(&expected, &run_text(&plan, "n\n007\n"), "one column, two types");
}

/// A template declared by its signature in one module and defined in another, loaded
/// in either order.
#[test]
fn signatures_defined_in_another_module() {
    let uses = format!(
        "{PREFIXES}
        ex:Helper [ ottr:IRI ?x ] .
        ex:Root [ ottr:IRI ?id ] :: {{ ex:Helper(?id) }} ."
    );
    let defines = format!("{PREFIXES} ex:Helper [ ottr:IRI ?x ] :: {{ ottr:Triple(?x, ex:p, ex:o) }} .");
    for order in [[&uses, &defines], [&defines, &uses]] {
        let mut lib = Library::new();
        for (i, doc) in order.iter().enumerate() {
            lib.add(parse_stottr(doc, &format!("module {i}")).unwrap());
        }
        let expected = turtle(&format!("{PREFIXES} ex:a ex:p ex:o ."));
        assert_same(&expected, &run_text(&compile(&lib, &["ex:Root"]), "id\nex:a\n"), "signature + definition");
    }
}

/// One's own module, built on the published library's templates.
#[test]
fn own_templates_on_a_published_library() {
    let mut lib = Library::load(&[fixture("retail/ottr")]).unwrap();
    lib.add(
        parse_stottr(
            &format!(
                "{PREFIXES}
                ex:GiftCard [ ! ottr:IRI ?id, ? xsd:decimal ?value, ? ?message ] :: {{
                    rt:Typed(?id, ex:GiftCard),
                    rt:Price(?id, ex:faceValue, ?value, cur:GBP),
                    rt:Value(?id, ex:message, ?message)
                }} ."
            ),
            "gift-cards",
        )
        .unwrap(),
    );
    let plan = compile(&lib, &["ex:GiftCard"]);
    let expected = turtle(&format!(
        "{PREFIXES}
        ex:g1 a ex:GiftCard ; ex:message \"Happy birthday\" ;
            ex:faceValue [ a schema:PriceSpecification ; schema:price 25.5 ; schema:priceCurrency cur:GBP ] .
        ex:g2 a ex:GiftCard ."
    ));
    assert_same(&expected, &run_text(&plan, "id,value,message\nex:g1,25.50,Happy birthday\nex:g2,,\n"), "own module");
}
