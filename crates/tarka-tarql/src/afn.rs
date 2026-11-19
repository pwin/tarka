//! Jena ARQ's function library (`afn:`), as TARQL queries written for Jena call it:
//! `afn:localname(?iri)`, `afn:strjoin(", ", ?a, ?b)` … Each does what Jena 4.8's does
//! (`org.apache.jena.sparql.function.library`), as TARQL 1.3 runs it; an argument it
//! cannot take leaves the result unbound, as an error in an expression does in SPARQL.
//!
//! Not here: `afn:bnode`, which gives a blank node's label (tarka makes those afresh on
//! every run), and `afn:sprintf`, which formats as Java's `String.format` does. A query
//! that calls either is refused before any input is read, as for any unknown function.

use oxrdf::vocab::xsd;
use oxrdf::{Literal, NamedNode, Term};
use sha1::{Digest, Sha1};
use spareval::QueryEvaluator;

use crate::pfunc::{number, split_xml};

/// Jena's namespaces for its functions: the current one, the one it had before it moved
/// to Apache, and the Java package that loads them directly.
pub const AFN_NAMESPACES: [&str; 3] =
    ["http://jena.apache.org/ARQ/function#", "http://jena.hpl.hp.com/ARQ/function#", "java:org.apache.jena.sparql.function.library."];

/// `evaluator` with Jena's functions.
pub fn register(mut evaluator: QueryEvaluator) -> QueryEvaluator {
    // the time the query started, as Jena has it: here, the run
    let now: Term = Literal::new_typed_literal(oxsdatatypes::DateTime::now().to_string(), xsd::DATE_TIME).into();
    for ns in AFN_NAMESPACES {
        let name = |local: &str| NamedNode::new_unchecked(format!("{ns}{local}"));
        let now = now.clone();
        evaluator = evaluator
            .with_custom_function(name("localname"), |args| iri_part(args, false))
            .with_custom_function(name("namespace"), |args| iri_part(args, true))
            .with_custom_function(name("strjoin"), strjoin)
            .with_custom_function(name("substr"), substr)
            .with_custom_function(name("substring"), substr)
            .with_custom_function(name("sha1sum"), sha1sum)
            .with_custom_function(name("min"), |args| pick(args, |a, b| a <= b))
            .with_custom_function(name("max"), |args| pick(args, |a, b| a >= b))
            .with_custom_function(name("pi"), |args| args.is_empty().then(|| double(std::f64::consts::PI)))
            .with_custom_function(name("e"), |args| args.is_empty().then(|| double(std::f64::consts::E)))
            .with_custom_function(name("sqrt"), |args| match args {
                [Term::Literal(l)] => number(l).map(|n| double(n.sqrt())),
                _ => None,
            })
            .with_custom_function(name("now"), move |args| args.is_empty().then(|| now.clone()));
    }
    evaluator
}

/// `afn:localname` and `afn:namespace`: the parts of an IRI either side of where Jena
/// splits it, as strings.
fn iri_part(args: &[Term], namespace: bool) -> Option<Term> {
    let [Term::NamedNode(iri)] = args else { return None };
    let (ns, local) = iri.as_str().split_at(split_xml(iri.as_str()));
    Some(Literal::new_simple_literal(if namespace { ns } else { local }).into())
}

/// `afn:strjoin(separator, string …)`: the lexical forms joined.
fn strjoin(args: &[Term]) -> Option<Term> {
    let (separator, strings) = args.split_first()?;
    let lexical = |t: &Term| match t {
        Term::Literal(l) => Some(l.value().to_owned()),
        _ => None,
    };
    let parts: Option<Vec<String>> = strings.iter().map(lexical).collect();
    Some(Literal::new_simple_literal(parts?.join(&lexical(separator)?)).into())
}

/// `afn:substr(string, start [, end])`: Java's `substring`, counting characters from 0.
fn substr(args: &[Term]) -> Option<Term> {
    let index = |t: &Term| match t {
        Term::Literal(l) => number(l).filter(|n| n.fract() == 0.0 && *n >= 0.0).map(|n| n as usize),
        _ => None,
    };
    let (Term::Literal(text), start, end) = (args.first()?, index(args.get(1)?)?, args.get(2)) else { return None };
    if args.len() > 3 {
        return None;
    }
    let chars: Vec<char> = text.value().chars().collect();
    let end = match end {
        Some(e) => index(e)?,
        None => chars.len(),
    };
    (start <= end && end <= chars.len()).then(|| Literal::new_simple_literal(chars[start..end].iter().collect::<String>()).into())
}

/// `afn:sha1sum`: the SHA-1 of a literal's lexical form or an IRI, in hex.
fn sha1sum(args: &[Term]) -> Option<Term> {
    let text = match args {
        [Term::Literal(l)] => l.value(),
        [Term::NamedNode(n)] => n.as_str(),
        _ => return None,
    };
    let hex: String = Sha1::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
    Some(Literal::new_simple_literal(hex).into())
}

/// `afn:min` and `afn:max` of two numbers: the one `keep_first` prefers, as it was given.
fn pick(args: &[Term], keep_first: fn(f64, f64) -> bool) -> Option<Term> {
    let [Term::Literal(a), Term::Literal(b)] = args else { return None };
    let (x, y) = (number(a)?, number(b)?);
    Some(if keep_first(x, y) { a.clone() } else { b.clone() }.into())
}

/// An `xsd:double`. (Written in canonical form, as tarka writes every typed value:
/// Jena's `2.0e0` is `2`.)
fn double(f: f64) -> Term {
    Literal::from(f).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lit(s: &str) -> Term {
        Literal::new_simple_literal(s).into()
    }

    fn int(n: i64) -> Term {
        Literal::from(n).into()
    }

    /// As Jena 4.8 gives them (TARQL 1.3, checked).
    #[test]
    fn functions_give_what_jena_gives() {
        let text = |t: Option<Term>| t.map(|t| t.to_string()).unwrap_or_default();
        let iri = Term::from(NamedNode::new_unchecked("http://example.com/ns#thing"));
        assert_eq!(text(iri_part(std::slice::from_ref(&iri), false)), "\"thing\"");
        assert_eq!(text(iri_part(&[iri], true)), "\"http://example.com/ns#\"");
        assert_eq!(text(strjoin(&[lit("-"), lit("a"), lit("b"), lit("c")])), "\"a-b-c\"");
        assert_eq!(text(strjoin(&[lit(","), lit("x"), int(1)])), "\"x,1\"");
        assert_eq!(text(substr(&[lit("abcdef"), int(1), int(3)])), "\"bc\"");
        assert_eq!(text(substr(&[lit("abcdef"), int(2)])), "\"cdef\"");
        assert_eq!(text(substr(&[lit("héllo wörld"), int(1), int(4)])), "\"éll\"");
        assert_eq!(substr(&[lit("abc"), int(2), int(5)]), None, "beyond the end");
        assert_eq!(text(sha1sum(&[lit("abc")])), "\"a9993e364706816aba3e25717850c26c9cd0d89d\"");
        assert_eq!(text(sha1sum(&[NamedNode::new_unchecked("http://ex/x").into()])), "\"feb59790c1716b2c5b6de322b6a137290abd4a02\"");
        let decimal: Term = Literal::new_typed_literal("2.5", xsd::DECIMAL).into();
        assert_eq!(pick(&[int(3), decimal.clone()], |a, b| a <= b), Some(decimal.clone()));
        assert_eq!(pick(&[int(3), decimal], |a, b| a >= b), Some(int(3)));
        let value = |t: Term| match t {
            Term::Literal(l) if l.datatype() == xsd::DOUBLE => l.value().parse::<f64>().unwrap(),
            other => panic!("not a double: {other}"),
        };
        assert_eq!(value(double(std::f64::consts::PI)), std::f64::consts::PI);
        assert_eq!(value(double(2f64.sqrt())), 2f64.sqrt());
        assert_eq!(value(double(2.0)), 2.0);
    }
}
