//! Keeps queries to SPARQL 1.1.
//!
//! spargebra parses SPARQL 1.2 when its `sparql-12` feature is on, and a build that
//! includes SHACL_Engine or HOLOS turns it on for every crate. So the RDF 1.1 scope is
//! checked here rather than left to the parser: triple terms, SPARQL 1.2 functions and
//! literals with a base direction are rejected whichever way spargebra was built.

use spargebra::algebra::{AggregateExpression, Expression, Function, GraphPattern, OrderExpression};
use spargebra::term::{GroundTerm, TermPattern, TriplePattern};

use crate::TarqlError;

const DIR_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString";

/// Fails on anything in a CONSTRUCT template or WHERE clause beyond SPARQL 1.1.
pub fn check(template: &[TriplePattern], pattern: &GraphPattern) -> Result<(), TarqlError> {
    for t in template {
        term(&t.subject)?;
        term(&t.object)?;
    }
    graph_pattern(pattern)
}

fn literal(l: &oxrdf::Literal) -> Result<(), TarqlError> {
    if l.datatype().as_str() == DIR_LANG_STRING {
        return Err(TarqlError::Unsupported("a literal with a base direction"));
    }
    Ok(())
}

fn term(t: &TermPattern) -> Result<(), TarqlError> {
    match t {
        TermPattern::NamedNode(_) | TermPattern::BlankNode(_) | TermPattern::Variable(_) => Ok(()),
        TermPattern::Literal(l) => literal(l),
        #[allow(unreachable_patterns)]
        _ => Err(TarqlError::Unsupported("an RDF 1.2 triple term")),
    }
}

fn ground(t: &GroundTerm) -> Result<(), TarqlError> {
    match t {
        GroundTerm::NamedNode(_) => Ok(()),
        GroundTerm::Literal(l) => literal(l),
        #[allow(unreachable_patterns)]
        _ => Err(TarqlError::Unsupported("an RDF 1.2 triple term")),
    }
}

fn graph_pattern(p: &GraphPattern) -> Result<(), TarqlError> {
    match p {
        GraphPattern::Bgp { patterns } => patterns.iter().try_for_each(|t| {
            term(&t.subject)?;
            term(&t.object)
        }),
        GraphPattern::Path { subject, object, .. } => {
            term(subject)?;
            term(object)
        }
        GraphPattern::Join { left, right } | GraphPattern::Union { left, right } | GraphPattern::Minus { left, right } => {
            graph_pattern(left)?;
            graph_pattern(right)
        }
        GraphPattern::LeftJoin { left, right, expression } => {
            graph_pattern(left)?;
            graph_pattern(right)?;
            expression.as_ref().map_or(Ok(()), expr)
        }
        GraphPattern::Filter { expr: e, inner } => {
            expr(e)?;
            graph_pattern(inner)
        }
        GraphPattern::Extend { inner, expression, .. } => {
            expr(expression)?;
            graph_pattern(inner)
        }
        GraphPattern::Values { bindings, .. } => bindings.iter().flatten().flatten().try_for_each(ground),
        GraphPattern::OrderBy { inner, expression } => {
            for o in expression {
                match o {
                    OrderExpression::Asc(e) | OrderExpression::Desc(e) => expr(e)?,
                }
            }
            graph_pattern(inner)
        }
        GraphPattern::Group { inner, aggregates, .. } => {
            for (_, a) in aggregates {
                if let AggregateExpression::FunctionCall { expr: e, .. } = a {
                    expr(e)?;
                }
            }
            graph_pattern(inner)
        }
        GraphPattern::Graph { inner, .. }
        | GraphPattern::Project { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner }
        | GraphPattern::Slice { inner, .. }
        | GraphPattern::Service { inner, .. } => graph_pattern(inner),
        #[allow(unreachable_patterns)]
        _ => Err(TarqlError::Unsupported("a SPARQL 1.2 graph pattern")),
    }
}

fn expr(e: &Expression) -> Result<(), TarqlError> {
    match e {
        Expression::NamedNode(_) | Expression::Variable(_) | Expression::Bound(_) => Ok(()),
        Expression::Literal(l) => literal(l),
        Expression::Or(a, b)
        | Expression::And(a, b)
        | Expression::Equal(a, b)
        | Expression::SameTerm(a, b)
        | Expression::Greater(a, b)
        | Expression::GreaterOrEqual(a, b)
        | Expression::Less(a, b)
        | Expression::LessOrEqual(a, b)
        | Expression::Add(a, b)
        | Expression::Subtract(a, b)
        | Expression::Multiply(a, b)
        | Expression::Divide(a, b) => {
            expr(a)?;
            expr(b)
        }
        Expression::In(a, list) => {
            expr(a)?;
            list.iter().try_for_each(expr)
        }
        Expression::UnaryPlus(a) | Expression::UnaryMinus(a) | Expression::Not(a) => expr(a),
        Expression::Exists(p) => graph_pattern(p),
        Expression::If(a, b, c) => {
            expr(a)?;
            expr(b)?;
            expr(c)
        }
        Expression::Coalesce(list) => list.iter().try_for_each(expr),
        Expression::FunctionCall(f, args) => {
            function(f)?;
            args.iter().try_for_each(expr)
        }
    }
}

fn function(f: &Function) -> Result<(), TarqlError> {
    match f {
        Function::Str
        | Function::Lang
        | Function::LangMatches
        | Function::Datatype
        | Function::Iri
        | Function::BNode
        | Function::Rand
        | Function::Abs
        | Function::Ceil
        | Function::Floor
        | Function::Round
        | Function::Concat
        | Function::SubStr
        | Function::StrLen
        | Function::Replace
        | Function::UCase
        | Function::LCase
        | Function::EncodeForUri
        | Function::Contains
        | Function::StrStarts
        | Function::StrEnds
        | Function::StrBefore
        | Function::StrAfter
        | Function::Year
        | Function::Month
        | Function::Day
        | Function::Hours
        | Function::Minutes
        | Function::Seconds
        | Function::Timezone
        | Function::Tz
        | Function::Now
        | Function::Uuid
        | Function::StrUuid
        | Function::Md5
        | Function::Sha1
        | Function::Sha256
        | Function::Sha384
        | Function::Sha512
        | Function::StrLang
        | Function::StrDt
        | Function::IsIri
        | Function::IsBlank
        | Function::IsLiteral
        | Function::IsNumeric
        | Function::Regex
        | Function::Custom(_) => Ok(()),
        #[allow(unreachable_patterns)]
        _ => Err(TarqlError::Unsupported("a SPARQL 1.2 function")),
    }
}
