//! Placing a VALUES table of rows first in a WHERE group.
//!
//! TARQL binds each row as a solution that the WHERE group starts from, as if
//! `VALUES (?col …) { (…) }` came first in it. spargebra has already turned the group
//! into algebra, where BINDs and FILTERs wrap the patterns they follow and joins fold
//! to the left. So the table goes in place of the group's left-most pattern.
//!
//! spargebra wraps a query's WHERE pattern in a projection of the variables it binds,
//! and in its solution modifiers. The table goes below those, and the projection is
//! widened to keep the table's variables (row columns used in the template).

use spargebra::algebra::GraphPattern;
use spargebra::term::Variable;

/// `pattern` with `values` first in its group, below the projection and any solution
/// modifiers.
pub fn with_values(pattern: GraphPattern, values: GraphPattern) -> GraphPattern {
    match pattern {
        GraphPattern::Project { inner, mut variables } => {
            if let GraphPattern::Values { variables: added, .. } = &values {
                for v in added {
                    if !variables.contains(v) {
                        variables.push(v.clone());
                    }
                }
            }
            GraphPattern::Project { inner: Box::new(with_values(*inner, values)), variables }
        }
        GraphPattern::Slice { inner, start, length } => GraphPattern::Slice { inner: Box::new(with_values(*inner, values)), start, length },
        GraphPattern::OrderBy { inner, expression } => GraphPattern::OrderBy { inner: Box::new(with_values(*inner, values)), expression },
        GraphPattern::Distinct { inner } => GraphPattern::Distinct { inner: Box::new(with_values(*inner, values)) },
        GraphPattern::Reduced { inner } => GraphPattern::Reduced { inner: Box::new(with_values(*inner, values)) },
        other => first_in_group(other, values),
    }
}

fn first_in_group(pattern: GraphPattern, values: GraphPattern) -> GraphPattern {
    match pattern {
        GraphPattern::Filter { expr, inner } => GraphPattern::Filter { expr, inner: Box::new(first_in_group(*inner, values)) },
        GraphPattern::Extend { inner, variable, expression } => {
            GraphPattern::Extend { inner: Box::new(first_in_group(*inner, values)), variable, expression }
        }
        GraphPattern::Join { left, right } => GraphPattern::Join { left: Box::new(first_in_group(*left, values)), right },
        GraphPattern::LeftJoin { left, right, expression } => {
            GraphPattern::LeftJoin { left: Box::new(first_in_group(*left, values)), right, expression }
        }
        GraphPattern::Minus { left, right } => GraphPattern::Minus { left: Box::new(first_in_group(*left, values)), right },
        GraphPattern::Bgp { patterns } if patterns.is_empty() => values,
        // a union, a sub-query, a graph pattern …: the rows join with all of it
        other => GraphPattern::Join { left: Box::new(values), right: Box::new(other) },
    }
}

/// The WHERE group itself, below the projection and solution modifiers.
fn group(pattern: &GraphPattern) -> &GraphPattern {
    match pattern {
        GraphPattern::Project { inner, .. }
        | GraphPattern::Slice { inner, .. }
        | GraphPattern::OrderBy { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner } => group(inner),
        other => other,
    }
}

/// Whether the pattern has solution modifiers at the top (`LIMIT`, `ORDER BY` …).
pub fn has_modifiers(pattern: &GraphPattern) -> bool {
    match pattern {
        GraphPattern::Project { inner, .. } => has_modifiers(inner),
        GraphPattern::Slice { .. } | GraphPattern::OrderBy { .. } | GraphPattern::Distinct { .. } | GraphPattern::Reduced { .. } => true,
        _ => false,
    }
}

/// The variables the group itself binds with BIND, which a row column of the same
/// name must not pre-empt.
pub fn bind_targets(pattern: &GraphPattern) -> Vec<Variable> {
    let mut out = Vec::new();
    collect_binds(group(pattern), &mut out);
    out
}

fn collect_binds(pattern: &GraphPattern, out: &mut Vec<Variable>) {
    match pattern {
        GraphPattern::Extend { inner, variable, .. } => {
            out.push(variable.clone());
            collect_binds(inner, out);
        }
        GraphPattern::Filter { inner, .. } | GraphPattern::Graph { inner, .. } => collect_binds(inner, out),
        GraphPattern::Join { left, right }
        | GraphPattern::LeftJoin { left, right, .. }
        | GraphPattern::Union { left, right }
        | GraphPattern::Minus { left, right } => {
            collect_binds(left, out);
            collect_binds(right, out);
        }
        _ => {} // sub-queries keep their own scope
    }
}
