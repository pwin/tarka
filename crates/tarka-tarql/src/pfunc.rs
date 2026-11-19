//! Jena ARQ's property functions, as TARQL queries written for Jena use them:
//! `?tag apf:strSplit (?tags ";")` binds `?tag` to each part of `?tags`.
//!
//! A property function is a triple pattern with a reserved predicate, and Jena evaluates
//! it where it stands in the WHERE group, with what the patterns before it have bound.
//! tarka evaluates rows against no data, where such a pattern would match nothing; so a
//! group with property functions in it is taken as steps ([`Staged`]): spareval evaluates
//! the patterns up to a property function, tarka applies the function to each solution,
//! and the rest of the group is evaluated on what it gives. Each function does what
//! Jena 4.8's does (`org.apache.jena.sparql.pfunction.library`), as TARQL 1.3 runs it.
//!
//! The ones that read RDF lists or containers in the data (`list:member`, `rdfs:member`,
//! `apf:bag` …), search a text index or report Jena's version have nothing to work on in
//! a row, and are refused, as is a property function anywhere but the top level of the
//! WHERE group: a query that relied on one would otherwise make nothing, and say nothing.

use holos_engine::{oxrdf, spargebra};
use std::collections::HashMap;

use oxrdf::vocab::{rdf, xsd};
use oxrdf::{Literal, NamedNode, NamedNodeRef, Term};
use regex::Regex;
use spargebra::algebra::{Expression, GraphPattern};
use spargebra::term::{NamedNodePattern, TermPattern, TriplePattern, Variable};

use crate::TarqlError;
use crate::inject;

/// Jena's namespaces for its property functions: the current one, the one it had before
/// it moved to Apache, and the Java package that loads them directly.
pub const APF_NAMESPACES: [&str; 3] =
    ["http://jena.apache.org/ARQ/property#", "http://jena.hpl.hp.com/ARQ/property#", "java:org.apache.jena.sparql.pfunction.library."];
const LIST_NAMESPACE: &str = "http://jena.apache.org/ARQ/list#";
const RDFS_MEMBER: &str = "http://www.w3.org/2000/01/rdf-schema#member";

const IN_THE_DATA: &str = "it reads the data, and a TARQL row has none";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    StrSplit,
    Concat,
    SplitIri,
    Str,
    Assign,
}

/// A side of a property function: one term, or a list of them.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    One(TermPattern),
    List(Vec<TermPattern>),
}

/// What a property function binds a variable to, for SHACL shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    String,
    Iri,
    /// Whatever the other side is.
    Any,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropertyFunction {
    pub kind: Kind,
    /// As a message names it: `apf:strSplit`.
    pub name: String,
    pub subject: Arg,
    pub object: Arg,
}

/// A solution, variable by variable.
pub type Bindings = HashMap<Variable, Term>;

/// What a predicate names: a property function tarka has, one it refuses (with why), or
/// neither.
enum Named {
    Function(Kind, String),
    Refused(String, &'static str),
    Not,
}

fn named(iri: &str) -> Named {
    if iri == RDFS_MEMBER {
        return Named::Refused("rdfs:member".into(), IN_THE_DATA);
    }
    if let Some(local) = iri.strip_prefix(LIST_NAMESPACE) {
        return Named::Refused(format!("list:{local}"), IN_THE_DATA);
    }
    let Some(local) = APF_NAMESPACES.iter().find_map(|ns| iri.strip_prefix(ns)) else { return Named::Not };
    let name = format!("apf:{local}");
    let kind = match local {
        "strSplit" => Kind::StrSplit,
        "concat" => Kind::Concat,
        "splitIRI" | "splitURI" => Kind::SplitIri,
        "str" => Kind::Str,
        "assign" => Kind::Assign,
        "bag" | "seq" | "alt" => return Named::Refused(name, IN_THE_DATA),
        "textMatch" => return Named::Refused(name, "it searches a text index"),
        "blankNode" | "bnode" => return Named::Refused(name, "it gives a blank node's label, which tarka makes afresh on every run"),
        "versionARQ" => return Named::Refused(name, "it gives the version of Jena's ARQ"),
        _ => return Named::Refused(name, "it is not one of Jena's property functions"),
    };
    Named::Function(kind, name)
}

/// The property functions of a basic graph pattern, in order, with their argument lists
/// read from the `rdf:first` and `rdf:rest` patterns that spell them; and the patterns
/// that are neither.
pub fn split_bgp(patterns: &[TriplePattern]) -> Result<(Vec<PropertyFunction>, Vec<TriplePattern>), TarqlError> {
    let mut used = vec![false; patterns.len()];
    let mut functions = Vec::new();
    for (i, t) in patterns.iter().enumerate() {
        let NamedNodePattern::NamedNode(p) = &t.predicate else { continue };
        let (kind, name) = match named(p.as_str()) {
            Named::Not => continue,
            Named::Refused(name, why) => return Err(TarqlError::PropertyFunction(format!("{name} is not supported: {why}"))),
            Named::Function(kind, name) => (kind, name),
        };
        used[i] = true;
        let subject = argument(&t.subject, patterns, &mut used);
        let object = argument(&t.object, patterns, &mut used);
        let f = PropertyFunction { kind, name, subject, object };
        f.check()?;
        functions.push(f);
    }
    let rest = patterns.iter().zip(&used).filter(|(_, u)| !**u).map(|(t, _)| t.clone()).collect();
    Ok((functions, rest))
}

/// A side of a property function: a list, if the term heads one spelled out in the
/// patterns, or else the term.
fn argument(t: &TermPattern, patterns: &[TriplePattern], used: &mut [bool]) -> Arg {
    let nil = |t: &TermPattern| matches!(t, TermPattern::NamedNode(n) if n.as_ref() == rdf::NIL);
    if nil(t) {
        return Arg::List(Vec::new());
    }
    let find = |node: &TermPattern, predicate: NamedNodeRef<'_>| {
        patterns
            .iter()
            .position(|p| &p.subject == node && matches!(&p.predicate, NamedNodePattern::NamedNode(n) if n.as_ref() == predicate))
    };
    let (mut items, mut spelling, mut node) = (Vec::new(), Vec::new(), t.clone());
    // a list is no longer than the patterns that spell it
    for _ in 0..patterns.len() {
        if !matches!(node, TermPattern::BlankNode(_)) {
            break;
        }
        let (Some(first), Some(rest)) = (find(&node, rdf::FIRST), find(&node, rdf::REST)) else { break };
        items.push(patterns[first].object.clone());
        spelling.extend([first, rest]);
        node = patterns[rest].object.clone();
        if nil(&node) {
            spelling.into_iter().for_each(|i| used[i] = true);
            return Arg::List(items);
        }
    }
    Arg::One(t.clone())
}

impl PropertyFunction {
    /// Fails on a property function written in a way Jena would refuse.
    fn check(&self) -> Result<(), TarqlError> {
        let usage = |usage: &str| Err(TarqlError::PropertyFunction(format!("{} is written {usage}", self.name)));
        match (self.kind, &self.subject, &self.object) {
            (Kind::StrSplit, Arg::One(_), Arg::List(args)) if args.len() == 2 => {
                // a regular expression written in the query is checked now
                if let TermPattern::Literal(re) = &args[1] {
                    Regex::new(re.value()).map_err(|e| TarqlError::PropertyFunction(format!("{}: {e}", self.name)))?;
                }
                Ok(())
            }
            (Kind::StrSplit, ..) => usage("?part apf:strSplit (?string \"regular expression\")"),
            (Kind::Concat, Arg::One(TermPattern::Variable(_)), _) => Ok(()),
            (Kind::Concat, ..) => usage("?string apf:concat (?a ?b …), with a variable for its subject"),
            (Kind::SplitIri, Arg::One(_), Arg::List(args)) if args.len() == 2 => Ok(()),
            (Kind::SplitIri, ..) => usage(&format!("?iri {} (?namespace ?localname)", self.name)),
            (Kind::Str, Arg::One(_), Arg::One(_)) | (Kind::Assign, Arg::One(_), Arg::One(_)) => Ok(()),
            (Kind::Str, ..) => usage("?string apf:str ?term"),
            (Kind::Assign, ..) => usage("?a apf:assign ?b"),
        }
    }

    /// The variables it binds, and to what (for SHACL shapes).
    pub fn outputs(&self) -> Vec<(&Variable, Output)> {
        fn var(t: &TermPattern) -> Option<&Variable> {
            match t {
                TermPattern::Variable(v) => Some(v),
                _ => None,
            }
        }
        let mut out = Vec::new();
        match (&self.kind, &self.subject, &self.object) {
            (Kind::StrSplit | Kind::Concat | Kind::Str, Arg::One(s), _) => out.extend(var(s).map(|v| (v, Output::String))),
            (Kind::SplitIri, _, Arg::List(args)) => {
                out.extend(var(&args[0]).map(|v| (v, Output::Iri)));
                out.extend(var(&args[1]).map(|v| (v, Output::String)));
            }
            (Kind::Assign, Arg::One(s), Arg::One(o)) => out.extend([var(s), var(o)].into_iter().flatten().map(|v| (v, Output::Any))),
            _ => {}
        }
        out
    }

    /// The solutions it makes of one: none, one or several. Errors are Jena's: an
    /// `apf:str` or `apf:assign` with nothing to work from, or a regular expression that
    /// is not one.
    pub fn apply(&self, mu: &Bindings, regexes: &mut HashMap<String, Regex>) -> Result<Vec<Bindings>, String> {
        let value = |t: &TermPattern| -> Option<Term> {
            match t {
                TermPattern::Variable(v) => mu.get(v).cloned(),
                TermPattern::NamedNode(n) => Some(n.clone().into()),
                TermPattern::Literal(l) => Some(l.clone().into()),
                _ => None,
            }
        };
        let unbound = |t: &TermPattern| matches!(t, TermPattern::Variable(v) if !mu.contains_key(v));
        // `target` given `term`: bound if a free variable, else kept only if it is the same
        let give = |target: &TermPattern, term: Term| -> Vec<Bindings> {
            match target {
                TermPattern::Variable(v) if !mu.contains_key(v) => {
                    let mut next = mu.clone();
                    next.insert(v.clone(), term);
                    vec![next]
                }
                t => value(t).filter(|t| *t == term).map(|_| mu.clone()).into_iter().collect(),
            }
        };
        Ok(match (self.kind, &self.subject, &self.object) {
            (Kind::StrSplit, Arg::One(s), Arg::List(args)) => {
                let (Some(Term::Literal(text)), Some(Term::Literal(re))) = (value(&args[0]), value(&args[1])) else {
                    return Ok(Vec::new());
                };
                if !regexes.contains_key(re.value()) {
                    let compiled = Regex::new(re.value()).map_err(|e| format!("{}: {e}", self.name))?;
                    regexes.insert(re.value().to_owned(), compiled);
                }
                let parts: Vec<&str> = java_split(text.value(), &regexes[re.value()]).into_iter().map(java_trim).collect();
                match (s, value(s)) {
                    (TermPattern::Variable(v), None) => parts
                        .into_iter()
                        .map(|p| {
                            let mut next = mu.clone();
                            next.insert(v.clone(), Literal::new_simple_literal(p).into());
                            next
                        })
                        .collect(),
                    // bound: kept if it is one of the parts
                    (_, Some(Term::Literal(l))) if l.datatype() == xsd::STRING && parts.contains(&l.value()) => vec![mu.clone()],
                    _ => Vec::new(),
                }
            }
            (Kind::Concat, Arg::One(s), object) => {
                let args = match object {
                    Arg::List(args) => args.as_slice(),
                    Arg::One(t) => std::slice::from_ref(t),
                };
                let mut text = String::new();
                for a in args {
                    match value(a) {
                        Some(Term::Literal(l)) => text.push_str(l.value()),
                        Some(Term::NamedNode(n)) => text.push_str(n.as_str()),
                        // unbound, or a blank node, whose label tarka makes afresh on every run
                        _ => return Ok(Vec::new()),
                    }
                }
                give(s, Literal::new_simple_literal(text).into())
            }
            (Kind::SplitIri, Arg::One(s), Arg::List(args)) => {
                let Some(Term::NamedNode(iri)) = value(s) else { return Ok(Vec::new()) };
                let at = split_xml(iri.as_str());
                let (namespace, localname) = iri.as_str().split_at(at);
                let Ok(namespace) = NamedNode::new(namespace) else { return Ok(Vec::new()) };
                let mut out = Vec::new();
                // the namespace: an IRI, or a string with its text
                let namespaced: Vec<Bindings> = match (&args[0], value(&args[0])) {
                    (_, None) => give(&args[0], namespace.clone().into()),
                    (_, Some(Term::NamedNode(n))) if n == namespace => vec![mu.clone()],
                    (_, Some(Term::Literal(l))) if l.datatype() == xsd::STRING && l.value() == namespace.as_str() => vec![mu.clone()],
                    _ => Vec::new(),
                };
                for m in namespaced {
                    let local = Literal::new_simple_literal(localname);
                    match &args[1] {
                        TermPattern::Variable(v) if !m.contains_key(v) => {
                            let mut next = m;
                            next.insert(v.clone(), local.into());
                            out.push(next);
                        }
                        TermPattern::Variable(v) => {
                            if m.get(v) == Some(&Term::from(local)) {
                                out.push(m);
                            }
                        }
                        t => {
                            if value(t) == Some(local.into()) {
                                out.push(m);
                            }
                        }
                    }
                }
                out
            }
            (Kind::Str, Arg::One(s), Arg::One(o)) => {
                if matches!(value(s), Some(Term::NamedNode(_) | Term::BlankNode(_))) || matches!(s, TermPattern::BlankNode(_)) {
                    return Ok(Vec::new());
                }
                let text = match value(o) {
                    _ if unbound(s) && unbound(o) => return Err(format!("{}: its subject and object are both unbound", self.name)),
                    None => return Err(format!("{}: its object is unbound", self.name)),
                    Some(Term::Literal(l)) => l.value().to_owned(),
                    Some(Term::NamedNode(n)) => n.as_str().to_owned(),
                    Some(_) => return Err(format!("{}: its object is a blank node", self.name)),
                };
                give(s, Literal::new_simple_literal(text).into())
            }
            (Kind::Assign, Arg::One(s), Arg::One(o)) => match (value(s), value(o)) {
                (None, None) => return Err(format!("{}: its subject and object are both unbound", self.name)),
                (None, Some(term)) => give(s, term),
                (Some(term), None) => give(o, term),
                (Some(a), Some(b)) => {
                    if same_value(&a, &b) {
                        vec![mu.clone()]
                    } else {
                        Vec::new()
                    }
                }
            },
            _ => Vec::new(),
        })
    }
}

/// Java's `String.split(regex)`, as Jena's `apf:strSplit` calls it: an empty string is one
/// empty part, a match of no width at the start makes no leading part, and the empty
/// parts at the end are dropped.
pub fn java_split<'a>(text: &'a str, re: &Regex) -> Vec<&'a str> {
    if text.is_empty() {
        return vec![""];
    }
    let (mut parts, mut last) = (Vec::new(), 0);
    for m in re.find_iter(text) {
        if m.end() == 0 {
            continue;
        }
        parts.push(&text[last..m.start()]);
        last = m.end();
    }
    parts.push(&text[last..]);
    while parts.last() == Some(&"") {
        parts.pop();
    }
    parts
}

/// Java's `String.trim()`: the characters up to U+0020 at either end.
pub fn java_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c <= ' ')
}

/// Where Jena splits an IRI into namespace and local name (`SplitIRI.splitXML`, which
/// `apf:splitIRI`, `afn:localname` and `afn:namespace` use): before the longest XML
/// NCName at its end that starts with a name start character, never inside a `%xx`
/// escape, and not straight after `mailto:`. The IRI's length when there is no local
/// name (`…/123`, `…/`).
pub fn split_xml(iri: &str) -> usize {
    let chars: Vec<(usize, char)> = iri.char_indices().collect();
    let len = chars.len();
    if len == 0 {
        return 0;
    }
    // back from the end over name characters, down to the second character at most
    let mut i = len - 1;
    while i >= 1 && is_ncname_char(chars[i].1) {
        i -= 1;
    }
    let mut j = i + 1;
    if j >= len {
        return iri.len();
    }
    // not inside a %-escape
    if j >= 2 && chars[j - 2].1 == '%' {
        j += 1;
    }
    if j >= 1 && chars[j - 1].1 == '%' {
        j += 2;
        if j > len {
            return iri.len();
        }
    }
    // forward to a name start character
    while j < len {
        if is_ncname_start(chars[j].1) && !(j == 7 && iri.starts_with("mailto:")) {
            break;
        }
        j += 1;
    }
    chars.get(j).map_or(iri.len(), |(at, _)| *at)
}

/// XML 1.1's NameStartChar, without the colon.
fn is_ncname_start(c: char) -> bool {
    matches!(c,
        'A'..='Z' | '_' | 'a'..='z' | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}'
        | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}' | '\u{200C}'..='\u{200D}' | '\u{2070}'..='\u{218F}'
        | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}' | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}'
        | '\u{10000}'..='\u{EFFFF}')
}

/// XML 1.1's NameChar, without the colon.
fn is_ncname_char(c: char) -> bool {
    is_ncname_start(c) || matches!(c, '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
}

/// Jena's `sameValueAs`, as `apf:assign` tests two bound sides: the same term, or numbers
/// of the same value.
fn same_value(a: &Term, b: &Term) -> bool {
    if a == b {
        return true;
    }
    match (a, b) {
        (Term::Literal(x), Term::Literal(y)) => matches!((number(x), number(y)), (Some(x), Some(y)) if x == y),
        _ => false,
    }
}

/// The value of a numeric literal.
pub fn number(l: &Literal) -> Option<f64> {
    let local = l.datatype().as_str().strip_prefix("http://www.w3.org/2001/XMLSchema#")?;
    let numeric = matches!(
        local,
        "integer"
            | "decimal"
            | "double"
            | "float"
            | "long"
            | "int"
            | "short"
            | "byte"
            | "nonNegativeInteger"
            | "positiveInteger"
            | "nonPositiveInteger"
            | "negativeInteger"
            | "unsignedLong"
            | "unsignedInt"
            | "unsignedShort"
            | "unsignedByte"
    );
    if !numeric {
        return None;
    }
    match l.value().trim() {
        "INF" | "+INF" => Some(f64::INFINITY),
        "-INF" => Some(f64::NEG_INFINITY),
        v => v.parse().ok(),
    }
}

/// A WHERE group with property functions in it, as steps to take in order.
#[derive(Clone, Debug)]
pub struct Staged {
    /// From the group's left-most pattern on.
    pub steps: Vec<Step>,
    /// The group's FILTER, which holds for all of it.
    pub filter: Option<Expression>,
}

#[derive(Clone, Debug)]
pub enum Step {
    Extend(Variable, Expression),
    Join(GraphPattern),
    LeftJoin(GraphPattern, Option<Expression>),
    Minus(GraphPattern),
    Function(PropertyFunction),
}

impl Step {
    /// `inner` with this step applied (a property function is not a SPARQL step).
    pub fn wrap(&self, inner: GraphPattern) -> GraphPattern {
        let inner = Box::new(inner);
        match self {
            Self::Extend(variable, expression) => {
                GraphPattern::Extend { inner, variable: variable.clone(), expression: expression.clone() }
            }
            Self::Join(right) => GraphPattern::Join { left: inner, right: Box::new(right.clone()) },
            Self::LeftJoin(right, expression) => {
                GraphPattern::LeftJoin { left: inner, right: Box::new(right.clone()), expression: expression.clone() }
            }
            Self::Minus(right) => GraphPattern::Minus { left: inner, right: Box::new(right.clone()) },
            Self::Function(_) => *inner,
        }
    }
}

/// The steps of a WHERE pattern's group, if it has property functions; checks each of
/// them, wherever it is, and that none is below the top level of the group.
pub fn staged(pattern: &GraphPattern) -> Result<Option<Staged>, TarqlError> {
    let all = count(pattern)?;
    if all == 0 {
        return Ok(None);
    }
    let (filter, chain) = match inject::group(pattern) {
        GraphPattern::Filter { expr, inner } => (Some(expr.clone()), inner.as_ref()),
        group => (None, group),
    };
    let mut steps = Vec::new();
    linear(chain, &mut steps)?;
    if steps.iter().filter(|s| matches!(s, Step::Function(_))).count() < all {
        return Err(TarqlError::PropertyFunction(
            "a property function (apf:…) is evaluated at the top level of the WHERE clause, \
             not inside OPTIONAL, UNION, MINUS, GRAPH, a sub-query or a nested group"
                .into(),
        ));
    }
    Ok(Some(Staged { steps, filter }))
}

fn linear(p: &GraphPattern, steps: &mut Vec<Step>) -> Result<(), TarqlError> {
    match p {
        GraphPattern::Extend { inner, variable, expression } => {
            linear(inner, steps)?;
            steps.push(Step::Extend(variable.clone(), expression.clone()));
        }
        GraphPattern::Join { left, right } => {
            linear(left, steps)?;
            part(right, steps)?;
        }
        GraphPattern::LeftJoin { left, right, expression } => {
            linear(left, steps)?;
            steps.push(Step::LeftJoin((**right).clone(), expression.clone()));
        }
        GraphPattern::Minus { left, right } => {
            linear(left, steps)?;
            steps.push(Step::Minus((**right).clone()));
        }
        other => part(other, steps)?,
    }
    Ok(())
}

/// One pattern of a group: a basic graph pattern's property functions become steps of
/// their own.
fn part(p: &GraphPattern, steps: &mut Vec<Step>) -> Result<(), TarqlError> {
    match p {
        GraphPattern::Bgp { patterns } => {
            let (functions, rest) = split_bgp(patterns)?;
            if !rest.is_empty() {
                steps.push(Step::Join(GraphPattern::Bgp { patterns: rest }));
            }
            steps.extend(functions.into_iter().map(Step::Function));
        }
        other => steps.push(Step::Join(other.clone())),
    }
    Ok(())
}

/// The property functions anywhere in a pattern, each checked.
fn count(p: &GraphPattern) -> Result<usize, TarqlError> {
    Ok(match p {
        GraphPattern::Bgp { patterns } => split_bgp(patterns)?.0.len(),
        GraphPattern::Join { left, right }
        | GraphPattern::LeftJoin { left, right, .. }
        | GraphPattern::Union { left, right }
        | GraphPattern::Minus { left, right } => count(left)? + count(right)?,
        GraphPattern::Filter { inner, .. }
        | GraphPattern::Extend { inner, .. }
        | GraphPattern::Graph { inner, .. }
        | GraphPattern::Service { inner, .. }
        | GraphPattern::Project { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner }
        | GraphPattern::Slice { inner, .. }
        | GraphPattern::OrderBy { inner, .. }
        | GraphPattern::Group { inner, .. } => count(inner)?,
        _ => 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(text: &str, re: &str) -> Vec<String> {
        java_split(text, &Regex::new(re).unwrap()).into_iter().map(|p| java_trim(p).to_owned()).collect()
    }

    /// As Jena 4.8 splits them (TARQL 1.3, checked).
    #[test]
    fn strings_split_as_java_splits_them() {
        assert_eq!(split("a;;b", ";"), ["a", "", "b"]);
        assert_eq!(split("a;b;", ";"), ["a", "b"]);
        assert_eq!(split("a;b; ", ";"), ["a", "b", ""]);
        assert_eq!(split(";a", ";"), ["", "a"]);
        assert_eq!(split("  x ; y  ", ";"), ["x", "y"]);
        assert_eq!(split(";;", ";"), Vec::<String>::new());
        assert_eq!(split("a", ";"), ["a"]);
        assert_eq!(split("", ";"), [""]);
        assert_eq!(split("a ; b|c ;d", r"\s*[;|]\s*"), ["a", "b", "c", "d"]);
        // a match of no width splits between characters, with no leading part
        assert_eq!(split("abc", ""), ["a", "b", "c"]);
    }

    /// As Jena 4.8 splits them (TARQL 1.3, checked).
    #[test]
    fn iris_split_before_an_xml_name() {
        let parts = |iri: &str| {
            let at = split_xml(iri);
            (iri[..at].to_owned(), iri[at..].to_owned())
        };
        assert_eq!(parts("http://example.com/ns#thing"), ("http://example.com/ns#".into(), "thing".into()));
        assert_eq!(parts("http://example.com/a/b"), ("http://example.com/a/".into(), "b".into()));
        assert_eq!(parts("http://example.com/item/123"), ("http://example.com/item/123".into(), "".into()));
        assert_eq!(parts("urn:isbn:0451450523"), ("urn:isbn:0451450523".into(), "".into()));
        assert_eq!(parts("mailto:me@example.com"), ("mailto:me@".into(), "example.com".into()));
        assert_eq!(parts("http://ex.org/a-1.b"), ("http://ex.org/".into(), "a-1.b".into()));
        assert_eq!(parts("http://example.com/"), ("http://example.com/".into(), "".into()));
        assert_eq!(parts("http://ex.org/p%20q"), ("http://ex.org/p%20".into(), "q".into()));
    }

    fn function(query: &str) -> PropertyFunction {
        let text = format!("PREFIX apf: <http://jena.apache.org/ARQ/property#> SELECT * WHERE {{ {query} }}");
        let spargebra::Query::Select { pattern, .. } = spargebra::SparqlParser::new().parse_query(&text).unwrap() else { unreachable!() };
        let Some(Staged { steps, .. }) = staged(&pattern).unwrap() else { panic!("no property function") };
        steps.into_iter().find_map(|s| if let Step::Function(f) = s { Some(f) } else { None }).unwrap()
    }

    fn apply(query: &str, given: &[(&str, Term)]) -> Result<Vec<Vec<(String, String)>>, String> {
        let mu: Bindings = given.iter().map(|(v, t)| (Variable::new_unchecked(*v), t.clone())).collect();
        let out = function(query).apply(&mu, &mut HashMap::new())?;
        Ok(out
            .into_iter()
            .map(|m| {
                let mut pairs: Vec<(String, String)> = m.into_iter().map(|(v, t)| (v.as_str().to_owned(), t.to_string())).collect();
                pairs.sort();
                pairs
            })
            .collect())
    }

    fn lit(s: &str) -> Term {
        Literal::new_simple_literal(s).into()
    }

    #[test]
    fn each_function_does_what_jena_does() {
        let pairs = |v: &[(&str, &str)]| v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
        // strSplit: a solution per part; bound, kept if one of them; arguments must be literals
        assert_eq!(apply("?p apf:strSplit (?s \";\")", &[("s", lit("a;b"))]).unwrap().len(), 2);
        assert_eq!(
            apply("?p apf:strSplit (?s \";\")", &[("s", lit("a;b")), ("p", lit("b"))]).unwrap(),
            [pairs(&[("p", "\"b\""), ("s", "\"a;b\"")])]
        );
        assert_eq!(apply("?p apf:strSplit (?s \";\")", &[("s", lit("a;b")), ("p", lit("c"))]).unwrap().len(), 0);
        assert_eq!(apply("?p apf:strSplit (?s \";\")", &[]).unwrap().len(), 0);
        // concat: lexical forms and IRIs; nothing if an argument is unbound
        assert_eq!(apply("?c apf:concat (\"x\" 1 <http://ex/y> \"z\"@en)", &[]).unwrap(), [pairs(&[("c", "\"x1http://ex/yz\"")])]);
        assert_eq!(apply("?c apf:concat (\"x\" ?missing)", &[]).unwrap().len(), 0);
        // splitIRI: the namespace an IRI, the local name a string; nothing for an unbound subject
        assert_eq!(
            apply("<http://example.com/ns#thing> apf:splitIRI (?ns ?ln)", &[]).unwrap(),
            [pairs(&[("ln", "\"thing\""), ("ns", "<http://example.com/ns#>")])]
        );
        assert_eq!(apply("?i apf:splitURI (?ns ?ln)", &[]).unwrap().len(), 0);
        assert_eq!(apply("<http://ex/a> apf:splitIRI (?ns \"a\")", &[]).unwrap(), [pairs(&[("ns", "<http://ex/>")])]);
        assert_eq!(apply("<http://ex/a> apf:splitIRI (?ns \"b\")", &[]).unwrap().len(), 0);
        // str and assign
        assert_eq!(apply("?s apf:str <http://ex/w>", &[]).unwrap(), [pairs(&[("s", "\"http://ex/w\"")])]);
        assert!(apply("?s apf:str ?o", &[]).is_err());
        assert_eq!(apply("?a apf:assign 42", &[]).unwrap(), [pairs(&[("a", "\"42\"^^<http://www.w3.org/2001/XMLSchema#integer>")])]);
        assert_eq!(apply("\"back\" apf:assign ?b", &[]).unwrap(), [pairs(&[("b", "\"back\"")])]);
        assert_eq!(apply("1 apf:assign 1.0", &[]).unwrap().len(), 1, "the same value");
        assert!(apply("?a apf:assign ?b", &[]).is_err());
    }

    #[test]
    fn what_cannot_be_evaluated_is_refused() {
        let error = |where_clause: &str| {
            let text = format!(
                "PREFIX apf: <http://jena.apache.org/ARQ/property#> PREFIX list: <http://jena.apache.org/ARQ/list#> SELECT * WHERE {{ {where_clause} }}"
            );
            let spargebra::Query::Select { pattern, .. } = spargebra::SparqlParser::new().parse_query(&text).unwrap() else {
                unreachable!()
            };
            staged(&pattern).unwrap_err().to_string()
        };
        assert!(error("?x list:member ?y").contains("list:member is not supported: it reads the data"));
        assert!(error("?x apf:textMatch \"x\"").contains("text index"));
        assert!(error("?x apf:nope ?y").contains("not one of Jena's property functions"));
        assert!(error("?p apf:strSplit (?s)").contains("apf:strSplit is written ?part apf:strSplit"));
        assert!(error("?p apf:strSplit (?s \"[\")").contains("apf:strSplit:"));
        assert!(error("OPTIONAL { ?p apf:strSplit (?s \";\") }").contains("top level of the WHERE clause"));
        // the old namespace is Jena's too
        let old = "PREFIX old: <http://jena.hpl.hp.com/ARQ/property#> SELECT * WHERE { ?p old:strSplit (?s \";\") }";
        let spargebra::Query::Select { pattern, .. } = spargebra::SparqlParser::new().parse_query(old).unwrap() else { unreachable!() };
        assert!(staged(&pattern).unwrap().is_some());
    }
}
