//! TARQL for tarka: a SPARQL CONSTRUCT query over CSV rows, compiled to a plan.
//!
//! The CONSTRUCT template becomes the plan's shape: one pattern per template triple,
//! and a fresh blank node per template blank node for every solution. The WHERE
//! clause becomes the plan's lifting, evaluated by spareval with each row's cells
//! bound by a VALUES table placed first in the group. That is TARQL's semantics, so
//! `BOUND(?column)` is true when the cell has a value. (oxi-gen substitutes cells
//! into the query instead, after which `BOUND(?column)` is false.)

pub mod eval;
pub mod inject;
pub mod scan;
pub mod sparql11;

use std::collections::HashMap;

use oxrdf::Variable;
use spargebra::term::{NamedNodePattern, TermPattern};
use spargebra::{Query, SparqlParser, SparqlSyntaxError};
use tarka_core::{BNodeId, Lifting, Pattern, Plan, PrefixMap, SparqlLifting, TermPat};
use thiserror::Error;

pub use eval::{EvalError, Solution, SparqlEvaluator, TARQL_NAMESPACES};

#[derive(Debug, Error)]
pub enum TarqlError {
    #[error("{0}")]
    Syntax(#[from] SparqlSyntaxError),
    #[error("a TARQL mapping is a CONSTRUCT query")]
    NotConstruct,
    #[error("{0} is not supported (tarka works with RDF 1.1 and SPARQL 1.1)")]
    Unsupported(&'static str),
}

fn parser() -> SparqlParser {
    SparqlParser::new().with_prefix("tarql", TARQL_NAMESPACES[0]).expect("a valid prefix IRI")
}

/// Compiles a TARQL query to a plan named `name`.
pub fn parse_tarql(text: &str, name: &str) -> Result<Plan, TarqlError> {
    let (prefixes, _) = scan::prologue(text);
    let Query::Construct { template, dataset, pattern, base_iri } = parser().parse_query(text)? else {
        return Err(TarqlError::NotConstruct);
    };
    sparql11::check(&template, &pattern)?;
    let mut plan = Plan::new(name, Lifting::Given);
    plan.prefixes = prefixes;
    let mut bnodes: HashMap<String, BNodeId> = HashMap::new();
    for triple in &template {
        let subject = term_pattern(&triple.subject, &mut plan, &mut bnodes)?;
        let predicate = match &triple.predicate {
            NamedNodePattern::NamedNode(n) => TermPat::Const(n.clone().into()),
            NamedNodePattern::Variable(v) => TermPat::Var(plan.var(v.as_str())),
        };
        let object = term_pattern(&triple.object, &mut plan, &mut bnodes)?;
        plan.root.patterns.push(Pattern { subject, predicate, object, requires: Vec::new() });
    }
    let mut made: Vec<BNodeId> = bnodes.into_values().collect();
    made.sort();
    plan.root.bnodes = made;
    plan.default_input = dataset.and_then(|d| d.default.first().map(|g| input_path(g.as_str())));
    let outputs = plan.vars.iter().enumerate().map(|(i, v)| (tarka_core::VarId(i), Variable::new_unchecked(&v.name))).collect();
    plan.lifting = Lifting::Sparql(Box::new(SparqlLifting {
        per_row: inject::has_modifiers(&pattern),
        pattern,
        base_iri: base_iri.map(oxiri::Iri::into_inner),
        variables: scan::variables(text),
        outputs,
    }));
    Ok(plan)
}

/// A SPARQL lifting from a WHERE group body (for OTTR root templates that carry
/// their lifting in `tq:` annotations). `outputs` names the plan variables to fill.
pub fn sparql_lifting(
    prefixes: &PrefixMap,
    base: Option<&str>,
    where_body: &str,
    modifiers: &str,
    outputs: Vec<(tarka_core::VarId, String)>,
) -> Result<SparqlLifting, TarqlError> {
    let mut text = String::new();
    if let Some(b) = base {
        text.push_str(&format!("BASE <{b}>\n"));
    }
    for (p, ns) in prefixes.iter() {
        text.push_str(&format!("PREFIX {p}: <{ns}>\n"));
    }
    text.push_str(&format!("CONSTRUCT {{}} WHERE {{\n{where_body}\n}} {modifiers}\n"));
    let Query::Construct { pattern, base_iri, .. } = parser().parse_query(&text)? else {
        return Err(TarqlError::NotConstruct);
    };
    sparql11::check(&[], &pattern)?;
    // the outputs are read from row columns too (a parameter that no BIND makes)
    let mut variables = scan::variables(&text);
    for (_, name) in &outputs {
        if !variables.contains(name) {
            variables.push(name.clone());
        }
    }
    Ok(SparqlLifting {
        per_row: inject::has_modifiers(&pattern),
        pattern,
        base_iri: base_iri.map(oxiri::Iri::into_inner),
        variables,
        outputs: outputs.into_iter().map(|(v, n)| (v, Variable::new_unchecked(n))).collect(),
    })
}

fn term_pattern(t: &TermPattern, plan: &mut Plan, bnodes: &mut HashMap<String, BNodeId>) -> Result<TermPat, TarqlError> {
    Ok(match t {
        TermPattern::NamedNode(n) => TermPat::Const(n.clone().into()),
        TermPattern::Literal(l) => TermPat::Const(l.clone().into()),
        TermPattern::Variable(v) => TermPat::Var(plan.var(v.as_str())),
        TermPattern::BlankNode(b) => {
            let id = match bnodes.get(b.as_str()) {
                Some(id) => *id,
                None => {
                    let id = plan.bnode();
                    bnodes.insert(b.as_str().to_owned(), id);
                    id
                }
            };
            TermPat::BNode(id)
        }
        #[allow(unreachable_patterns)]
        _ => return Err(TarqlError::Unsupported("an RDF 1.2 triple term")),
    })
}

/// A file path for a `FROM <…>` IRI.
fn input_path(iri: &str) -> String {
    iri.strip_prefix("file://").or_else(|| iri.strip_prefix("file:")).unwrap_or(iri).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tarka_core::{Emitter, Labels, Record, Value};

    fn run(query: &str, columns: &[&str], rows: &[&[Option<&str>]]) -> Vec<String> {
        let plan = parse_tarql(query, "q").unwrap();
        let Lifting::Sparql(lifting) = &plan.lifting else { panic!() };
        let columns: Vec<String> = columns.iter().map(|c| c.to_string()).collect();
        let eval = SparqlEvaluator::new(lifting, &plan.prefixes, &columns);
        eval.check().unwrap();
        let records: Vec<Record> =
            rows.iter().enumerate().map(|(i, r)| Record { row: i as u64, cells: r.iter().map(|c| c.map(Into::into)).collect() }).collect();
        let mut out = Vec::new();
        let mut emitter = Emitter::new(&plan);
        for (record, solutions) in records.iter().zip(eval.evaluate(&records, 0).unwrap()) {
            let mut labels = Labels::new(format!("b{}x", record.row));
            for solution in solutions {
                let mut env: Vec<Option<Value>> = vec![None; plan.vars.len()];
                for ((var, _), value) in lifting.outputs.iter().zip(solution) {
                    env[var.0] = value.map(Value::Term);
                }
                emitter.emit(&mut env, &mut labels, &mut |t| out.push(t.to_string()));
            }
        }
        out.sort();
        out
    }

    #[test]
    fn rows_bind_like_tarql() {
        let q = "PREFIX ex: <http://ex/>\nCONSTRUCT { ?s ex:name ?name ; ex:has ?flag ; ex:row ?ROWNUM ; ex:node [ ex:v ?name ] } \
                 WHERE { BIND(tarql:expandPrefixedName(?id) AS ?s) BIND(BOUND(?name) AS ?flag) }";
        let out = run(q, &["id", "name"], &[&[Some("ex:a"), Some("A")], &[Some("ex:b"), None]]);
        assert_eq!(
            out,
            [
                "<http://ex/a> <http://ex/has> \"true\"^^<http://www.w3.org/2001/XMLSchema#boolean>",
                "<http://ex/a> <http://ex/name> \"A\"",
                "<http://ex/a> <http://ex/node> _:b0x1",
                "<http://ex/a> <http://ex/row> \"0\"^^<http://www.w3.org/2001/XMLSchema#integer>",
                "<http://ex/b> <http://ex/has> \"false\"^^<http://www.w3.org/2001/XMLSchema#boolean>",
                "<http://ex/b> <http://ex/node> _:b1x1",
                "<http://ex/b> <http://ex/row> \"1\"^^<http://www.w3.org/2001/XMLSchema#integer>",
                "_:b0x1 <http://ex/v> \"A\"",
            ]
        );
    }

    #[test]
    fn filter_bound_and_column_named_like_a_bind() {
        // FILTER(BOUND(?id)) aborts oxi-gen; here it skips the row without an id. The
        // column `s` is ignored in favour of the BIND of ?s.
        let q = "PREFIX ex: <http://ex/>\nCONSTRUCT { ?s ex:p ?id } WHERE { BIND(IRI(CONCAT(str(ex:), ?id)) AS ?s) FILTER(BOUND(?id)) }";
        let out = run(q, &["id", "s"], &[&[Some("a"), Some("junk")], &[None, Some("junk")]]);
        assert_eq!(out, ["<http://ex/a> <http://ex/p> \"a\""]);
    }

    #[test]
    fn limits_apply_per_row() {
        let q = "PREFIX ex: <http://ex/>\nCONSTRUCT { ex:s ex:v ?v } WHERE { VALUES ?v { 1 2 } } LIMIT 1";
        assert_eq!(run(q, &["x"], &[&[Some("1")], &[Some("2")]]).len(), 1, "one distinct triple, once per row");
        let plan = parse_tarql(q, "q").unwrap();
        let Lifting::Sparql(l) = &plan.lifting else { panic!() };
        assert!(l.per_row);
    }

    #[test]
    fn errors() {
        assert!(matches!(parse_tarql("SELECT * WHERE {}", "q"), Err(TarqlError::NotConstruct)));
        assert!(matches!(parse_tarql("CONSTRUCT { ?s ?p }", "q"), Err(TarqlError::Syntax(_))));
        let plan = parse_tarql(
            "PREFIX x: <http://www.w3.org/2001/XMLSchema#> CONSTRUCT { <http://s> <http://p> ?d } WHERE { BIND(x:date(?v) AS ?d) }",
            "q",
        )
        .unwrap();
        let Lifting::Sparql(l) = &plan.lifting else { panic!() };
        let check = SparqlEvaluator::new(l, &plan.prefixes, &["v".to_owned()]).check();
        assert!(check.is_err(), "{check:?}");
    }

    /// SPARQL 1.2 is refused, whether or not spargebra was built to parse it (a build
    /// with SHACL_Engine turns its `sparql-12` feature on).
    #[test]
    fn sparql_12_is_refused() {
        let queries = [
            "CONSTRUCT { <http://s> <http://p> <<( <http://a> <http://b> <http://c> )>> } WHERE { }",
            "CONSTRUCT { <http://s> <http://p> ?t } WHERE { BIND(<<( <http://a> <http://b> ?c )>> AS ?t) }",
            "CONSTRUCT { <http://s> <http://p> ?t } WHERE { BIND(TRIPLE(<http://a>, <http://b>, ?c) AS ?t) }",
            "CONSTRUCT { <http://s> <http://p> ?d } WHERE { BIND(LANGDIR(?c) AS ?d) }",
            "CONSTRUCT { <http://s> <http://p> \"hi\"@en--ltr } WHERE { }",
            "CONSTRUCT { <http://s> <http://p> ?v } WHERE { VALUES ?v { \"hi\"@en--rtl } }",
        ];
        for q in queries {
            match parse_tarql(q, "q") {
                Err(TarqlError::Syntax(_) | TarqlError::Unsupported(_)) => {}
                other => panic!("{q}: {other:?}"),
            }
        }
        let tq = sparql_lifting(&PrefixMap::new(), None, "BIND(TRIPLE(<http://a>, <http://b>, ?c) AS ?t)", "", Vec::new());
        assert!(matches!(tq, Err(TarqlError::Syntax(_) | TarqlError::Unsupported(_))), "{tq:?}");
    }
}
