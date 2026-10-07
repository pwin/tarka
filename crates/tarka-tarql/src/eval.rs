//! Evaluating a SPARQL lifting over rows with spareval.

use std::collections::HashMap;
use std::sync::Arc;

use oxrdf::{BlankNode, Dataset, Literal, NamedNode, Term, Variable};
use spareval::{QueryEvaluationError, QueryEvaluator, QueryResults};
use spargebra::Query;
use spargebra::algebra::GraphPattern;
use spargebra::term::GroundTerm;
use tarka_core::{Cell, PrefixMap, Record, SparqlLifting};
use thiserror::Error;

use crate::inject::{bind_targets, with_values};

/// The namespaces of TARQL's functions: oxi-gen's and Jena TARQL's.
pub const TARQL_NAMESPACES: [&str; 2] = ["https://semanticarts.com/tarql/", "http://tarql.github.io/tarql#"];

/// The values of the plan's output variables for one solution.
pub type Solution = Vec<Option<Term>>;

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("rows {first}–{last}: {source}")]
    Rows { first: u64, last: u64, source: QueryEvaluationError },
}

pub struct SparqlEvaluator {
    evaluator: QueryEvaluator,
    pattern: GraphPattern,
    base_iri: Option<oxiri::Iri<String>>,
    outputs: Vec<Variable>,
    /// (index in a record, variable) for the row columns the query uses.
    columns: Vec<(usize, Variable)>,
    rownum: Option<Variable>,
    row_var: Variable,
    per_row: bool,
}

impl SparqlEvaluator {
    /// An evaluator for `lifting` over records with these `columns`.
    pub fn new(lifting: &SparqlLifting, prefixes: &PrefixMap, columns: &[String]) -> Self {
        let prefixes = Arc::new(prefixes.clone());
        let mut evaluator = QueryEvaluator::new();
        for ns in TARQL_NAMESPACES {
            let p = Arc::clone(&prefixes);
            evaluator = evaluator.with_custom_function(NamedNode::new_unchecked(format!("{ns}expandPrefixedName")), move |args| {
                expand_prefixed_name(&p, args)
            });
            let p = Arc::clone(&prefixes);
            evaluator =
                evaluator.with_custom_function(NamedNode::new_unchecked(format!("{ns}expandPrefix")), move |args| expand_prefix(&p, args));
        }
        let mentioned: Vec<&str> = lifting.variables.iter().map(String::as_str).collect();
        let bound_by_query: Vec<String> = bind_targets(&lifting.pattern).iter().map(|v| v.as_str().to_owned()).collect();
        let usable = |name: &str| mentioned.contains(&name) && !bound_by_query.iter().any(|b| b == name);
        let column_vars: Vec<(usize, Variable)> =
            columns.iter().enumerate().filter(|(_, c)| usable(c)).map(|(i, c)| (i, Variable::new_unchecked(c))).collect();
        let rownum = (usable("ROWNUM") && !columns.iter().any(|c| c == "ROWNUM")).then(|| Variable::new_unchecked("ROWNUM"));
        Self {
            evaluator,
            pattern: lifting.pattern.clone(),
            base_iri: lifting.base_iri.as_ref().and_then(|b| oxiri::Iri::parse(b.clone()).ok()),
            outputs: lifting.outputs.iter().map(|(_, v)| v.clone()).collect(),
            columns: column_vars,
            rownum,
            row_var: Variable::new_unchecked("__tarka_row"),
            per_row: lifting.per_row,
        }
    }

    /// Whether rows must be evaluated one at a time (solution modifiers apply per row).
    pub fn per_row(&self) -> bool {
        self.per_row
    }

    /// The solutions of each record, in record order. `first` is the position of the
    /// first record in the whole input; blank nodes made by `BNODE()` get labels from it.
    pub fn evaluate(&self, records: &[Record], first: u64) -> Result<Vec<Vec<Solution>>, EvalError> {
        let mut out: Vec<Vec<Solution>> = vec![Vec::new(); records.len()];
        if records.is_empty() {
            return Ok(out);
        }
        let failed = |source| EvalError::Rows { first: records[0].row, last: records[records.len() - 1].row, source };
        let mut variables = vec![self.row_var.clone()];
        variables.extend(self.columns.iter().map(|(_, v)| v.clone()));
        variables.extend(self.rownum.clone());
        let bindings = records
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let mut row: Vec<Option<GroundTerm>> = vec![Some(GroundTerm::Literal(Literal::from(i as i64)))];
                row.extend(self.columns.iter().map(|(c, _)| {
                    r.cells
                        .get(*c)
                        .and_then(Option::as_ref)
                        .and_then(Cell::text)
                        .map(|s| GroundTerm::Literal(Literal::new_simple_literal(s)))
                }));
                if self.rownum.is_some() {
                    row.push(Some(GroundTerm::Literal(Literal::from(r.row as i64))));
                }
                row
            })
            .collect();
        let pattern = with_values(self.pattern.clone(), GraphPattern::Values { variables, bindings });
        let query = Query::Select { dataset: None, pattern, base_iri: self.base_iri.clone() };
        let dataset = Dataset::new();
        let QueryResults::Solutions(solutions) = self.evaluator.prepare(&query).execute(&dataset).map_err(failed)? else {
            unreachable!("a SELECT query gives solutions")
        };
        for solution in solutions {
            let solution = solution.map_err(failed)?;
            let Some(Term::Literal(index)) = solution.get(&self.row_var) else { continue };
            let Ok(index) = index.value().parse::<usize>() else { continue };
            out[index].push(self.outputs.iter().map(|v| solution.get(v).cloned()).collect());
        }
        for (i, solutions) in out.iter_mut().enumerate() {
            stable_blank_nodes(first + i as u64, solutions);
        }
        Ok(out)
    }

    /// Checks that the query can be evaluated (for example that every function it
    /// calls exists), before any input is read.
    pub fn check(&self) -> Result<(), EvalError> {
        let probe = Record { row: 0, cells: vec![None; self.columns.iter().map(|(i, _)| i + 1).max().unwrap_or(0)] };
        self.evaluate(std::slice::from_ref(&probe), 0).map(|_| ())
    }
}

/// spareval's `BNODE()` makes random labels; give them stable ones per record so that
/// the output does not change from run to run.
fn stable_blank_nodes(record: u64, solutions: &mut [Solution]) {
    let mut renamed: HashMap<BlankNode, BlankNode> = HashMap::new();
    for solution in solutions {
        for term in solution.iter_mut().flatten() {
            if let Term::BlankNode(b) = term {
                let n = renamed.len() + 1;
                let stable = renamed.entry(b.clone()).or_insert_with(|| BlankNode::new_unchecked(format!("v{record}x{n}")));
                *term = stable.clone().into();
            }
        }
    }
}

fn expand_prefixed_name(prefixes: &PrefixMap, args: &[Term]) -> Option<Term> {
    let [Term::Literal(name)] = args else { return None };
    NamedNode::new(prefixes.expand(name.value())?).ok().map(Into::into)
}

fn expand_prefix(prefixes: &PrefixMap, args: &[Term]) -> Option<Term> {
    let [Term::Literal(prefix)] = args else { return None };
    prefixes.get(prefix.value()).map(|ns| Literal::new_simple_literal(ns).into())
}
