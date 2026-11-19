//! Keeps queries to SPARQL 1.1.
//!
//! tarka is built on HOLOS's crates, which turn on spargebra's `sparql-12` feature (and
//! the other Oxigraph crates' RDF 1.2 features) for the whole build, so the parser reads
//! SPARQL 1.2. The RDF 1.1 scope is therefore checked here rather than left to the
//! parser: triple terms, SPARQL 1.2 functions and literals with a base direction are
//! rejected, and every test run checks that against a parser that accepts them.

use holos_engine::{oxrdf, spargebra};
use oxrdf::NamedNode;
use spargebra::algebra::{AggregateExpression, Expression, Function, GraphPattern, OrderExpression};
use spargebra::term::{GroundTerm, TermPattern, TriplePattern};

use crate::TarqlError;

const DIR_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString";

/// Fails on anything in a CONSTRUCT template or WHERE clause beyond SPARQL 1.1.
pub fn check(template: &[TriplePattern], pattern: &GraphPattern) -> Result<(), TarqlError> {
    check_with(template, pattern, &|_| Ok(()))
}

/// [`check`], for a query tarka evaluates itself rather than sends to an endpoint: it also
/// fails on a function tarka would not evaluate as Jena does ([`crate::afn::refused`]).
pub fn check_local(template: &[TriplePattern], pattern: &GraphPattern) -> Result<(), TarqlError> {
    check_with(template, pattern, &crate::afn::refused)
}

/// What a custom function's IRI is checked with.
type Custom<'a> = &'a dyn Fn(&NamedNode) -> Result<(), TarqlError>;

fn check_with(template: &[TriplePattern], pattern: &GraphPattern, c: Custom) -> Result<(), TarqlError> {
    for t in template {
        term(&t.subject)?;
        term(&t.object)?;
    }
    graph_pattern(pattern, c)
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

fn graph_pattern(p: &GraphPattern, c: Custom) -> Result<(), TarqlError> {
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
            graph_pattern(left, c)?;
            graph_pattern(right, c)
        }
        GraphPattern::LeftJoin { left, right, expression } => {
            graph_pattern(left, c)?;
            graph_pattern(right, c)?;
            expression.as_ref().map_or(Ok(()), |e| expr(e, c))
        }
        GraphPattern::Filter { expr: e, inner } => {
            expr(e, c)?;
            graph_pattern(inner, c)
        }
        GraphPattern::Extend { inner, expression, .. } => {
            expr(expression, c)?;
            graph_pattern(inner, c)
        }
        GraphPattern::Values { bindings, .. } => bindings.iter().flatten().flatten().try_for_each(ground),
        GraphPattern::OrderBy { inner, expression } => {
            for o in expression {
                match o {
                    OrderExpression::Asc(e) | OrderExpression::Desc(e) => expr(e, c)?,
                }
            }
            graph_pattern(inner, c)
        }
        GraphPattern::Group { inner, aggregates, .. } => {
            for (_, a) in aggregates {
                if let AggregateExpression::FunctionCall { expr: e, .. } = a {
                    expr(e, c)?;
                }
            }
            graph_pattern(inner, c)
        }
        GraphPattern::Graph { inner, .. }
        | GraphPattern::Project { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner }
        | GraphPattern::Slice { inner, .. }
        | GraphPattern::Service { inner, .. } => graph_pattern(inner, c),
        #[allow(unreachable_patterns)]
        _ => Err(TarqlError::Unsupported("a SPARQL 1.2 graph pattern")),
    }
}

fn expr(e: &Expression, c: Custom) -> Result<(), TarqlError> {
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
            expr(a, c)?;
            expr(b, c)
        }
        Expression::In(a, list) => {
            expr(a, c)?;
            list.iter().try_for_each(|e| expr(e, c))
        }
        Expression::UnaryPlus(a) | Expression::UnaryMinus(a) | Expression::Not(a) => expr(a, c),
        Expression::Exists(p) => graph_pattern(p, c),
        Expression::If(a, b, d) => {
            expr(a, c)?;
            expr(b, c)?;
            expr(d, c)
        }
        Expression::Coalesce(list) => list.iter().try_for_each(|e| expr(e, c)),
        Expression::FunctionCall(f, args) => {
            function(f, c)?;
            args.iter().try_for_each(|e| expr(e, c))
        }
    }
}

fn function(f: &Function, c: Custom) -> Result<(), TarqlError> {
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
        | Function::Regex => Ok(()),
        Function::Custom(name) => c(name),
        #[allow(unreachable_patterns)]
        _ => Err(TarqlError::Unsupported("a SPARQL 1.2 function")),
    }
}
