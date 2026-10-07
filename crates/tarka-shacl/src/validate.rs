//! Validation with SHACL_Engine.

use oxrdf::{Graph, Term, TermRef, Triple};
use shacl::{GraphBuilder, TermId, TermStore, Vocab};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("the shapes cannot be used: {0}")]
pub struct ValidationError(String);

/// One validation result, its terms written as in N-Triples.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub focus_node: String,
    pub path: Option<String>,
    pub value: Option<String>,
    /// The constraint component, as `sh:MinCountConstraintComponent` names it.
    pub component: String,
    pub severity: String,
    pub shape: Option<String>,
    pub messages: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Validation {
    pub conforms: bool,
    pub findings: Vec<Finding>,
    /// The SHACL validation report.
    pub report: Graph,
}

/// Validates `data` against the shapes graph `shapes`.
pub fn validate<'a>(
    data: impl IntoIterator<Item = &'a Triple>,
    shapes: impl IntoIterator<Item = &'a Triple>,
) -> Result<Validation, ValidationError> {
    let mut store = TermStore::new();
    let vocab = Vocab::new(&mut store);
    // blank node labels are scoped per graph
    let data = graph(data, 0, &mut store);
    let shapes_graph = graph(shapes, 1, &mut store);
    let compiled = shacl::shapes::Shapes::compile(&shapes_graph, &store, &vocab).map_err(|e| ValidationError(e.to_string()))?;
    let report =
        shacl::validate::validate_in(&data, &compiled, &shapes_graph, &mut store, &vocab).map_err(|e| ValidationError(e.to_string()))?;
    let term = |t: TermId| store.to_oxrdf(t).to_string();
    let text = |t: TermId| match store.to_oxrdf(t) {
        Term::Literal(l) => l.value().to_owned(),
        other => other.to_string(),
    };
    let findings = report
        .results
        .iter()
        .map(|r| Finding {
            focus_node: term(r.focus_node),
            path: r.path.map(term),
            value: r.value.map(term),
            component: term(r.source_constraint_component),
            severity: term(r.severity),
            shape: r.source_shape.map(term),
            messages: r.messages.iter().map(|m| text(*m)).collect(),
        })
        .collect();
    Ok(Validation {
        conforms: report.conforms(&[], &vocab),
        findings,
        report: report.to_oxrdf(&store, &vocab, &shapes_graph, &compiled, &[]),
    })
}

fn graph<'a>(triples: impl IntoIterator<Item = &'a Triple>, scope: u32, store: &mut TermStore) -> shacl::Graph {
    let mut b = GraphBuilder::new();
    for t in triples {
        let s = store.intern_oxrdf(TermRef::from(t.subject.as_ref()), scope);
        let p = store.named_node(t.predicate.as_str());
        let o = store.intern_oxrdf(t.object.as_ref(), scope);
        b.push(s, p, o);
    }
    b.build()
}
