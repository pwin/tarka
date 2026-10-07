//! Values to arguments, the way Lutra's argument maps make them.
//!
//! In order, for one value and its argument map:
//!
//! 1. a missing value (NULL, unbound) is the map's `ottr:nullValue` (`ottr:none`);
//! 2. a value starting with the labelled blank prefix (`_:`) is that blank node, one node
//!    for the label throughout a run;
//! 3. a value in `ottr:blankNodeFresh` is a new blank node;
//! 4. a value in the translation table is its entry's term (a blank node: a new one);
//! 5. otherwise the value is read as the map's type: a list type splits it into items
//!    (`(a,b,(c,d))`, nesting allowed), an IRI type expands a prefixed name with the
//!    bOTTR file's prefixes, a datatype makes a literal of it (which must be valid), and
//!    `ottr:languageTag`, `ottr:languageTagSep` and `ottr:datatypeSep` make tagged or
//!    typed literals. Without a type, text is a plain literal, and an RDF term is kept
//!    as it is.
//!
//! An RDF term whose own type fits the map's type is kept; otherwise its lexical form
//! (or IRI) is read as text would be.

use std::collections::HashMap;
use std::str::FromStr;

use oxrdf::vocab::{rdf, xsd};
use oxrdf::{BlankNode, Literal, NamedNode, Term};
use tarka_core::{PrefixMap, TypeRef, Value};
use tarka_ottr::Param;
use tarka_ottr::types::{self, LITERAL, RESOURCE, basic, compatible, display, effective};

use crate::map::ArgumentMap;

/// A value from a source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Raw {
    Null,
    Text(String),
    Term(Term),
}

/// Makes arguments, keeping the blank nodes of a run apart.
#[derive(Default)]
pub struct Converter {
    fresh: u64,
    labelled: HashMap<String, BlankNode>,
    data: HashMap<BlankNode, BlankNode>,
}

impl Converter {
    fn fresh(&mut self) -> Value {
        self.fresh += 1;
        Value::Term(BlankNode::new_unchecked(format!("f{}", self.fresh)).into())
    }

    /// The argument for a value; `Ok(None)` is `ottr:none`.
    pub fn argument(&mut self, raw: &Raw, map: &ArgumentMap, prefixes: &PrefixMap) -> Result<Option<Value>, String> {
        let text = match raw {
            Raw::Null => return Ok(map.null_value.clone().map(Value::Term)),
            Raw::Text(t) => t.clone(),
            Raw::Term(t) => text_of(t),
        };
        let is_blank = matches!(raw, Raw::Term(Term::BlankNode(_)));
        if !is_blank && let Some(label) = text.strip_prefix(map.labelled_blank_prefix.as_str()).filter(|l| !l.is_empty()) {
            let next = self.labelled.len();
            let node = self.labelled.entry(label.to_owned()).or_insert_with(|| BlankNode::new_unchecked(format!("c{next}")));
            return Ok(Some(Value::Term(node.clone().into())));
        }
        if map.fresh_blank.contains(&text) {
            return Ok(Some(self.fresh()));
        }
        let as_term = match raw {
            Raw::Term(t) => t.clone(),
            _ => Literal::new_simple_literal(&text).into(),
        };
        if let Some((_, out)) = map.translation.iter().find(|(input, _)| *input == as_term) {
            return Ok(match out {
                None => None,
                Some(Term::BlankNode(_)) => Some(self.fresh()),
                Some(t) => Some(Value::Term(t.clone())),
            });
        }
        let ty = map.ty.clone().unwrap_or_else(|| basic(if matches!(raw, Raw::Term(_)) { RESOURCE } else { LITERAL }));
        if let Some(element) = list_element(&ty) {
            let items = parse_list(&text, map.list_start, map.list_end, &map.list_sep)?;
            return self.list(&items, &element, map, prefixes, raw).map(Some);
        }
        let ty = without_lub(&ty);
        match raw {
            Raw::Term(t) if map.language_tag.is_none() && map.language_tag_sep.is_none() && map.datatype_sep.is_none() => {
                if types::subtype(&intrinsic(t), &ty) {
                    return Ok(Some(Value::Term(self.data_term(t))));
                }
                to_term(&text, &ty, map, prefixes).map(|t| Some(Value::Term(t)))
            }
            _ => literal_term(&text, &ty, map, prefixes).map(|t| Some(Value::Term(t))),
        }
    }

    /// A blank node from the data, renamed apart from the run's other blank nodes.
    fn data_term(&mut self, t: &Term) -> Term {
        match t {
            Term::BlankNode(b) => {
                let next = self.data.len();
                self.data.entry(b.clone()).or_insert_with(|| BlankNode::new_unchecked(format!("d{next}"))).clone().into()
            }
            other => other.clone(),
        }
    }

    fn list(&mut self, items: &[ListItem], element: &TypeRef, map: &ArgumentMap, prefixes: &PrefixMap, raw: &Raw) -> Result<Value, String> {
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            out.push(match item {
                ListItem::List(inner) => self.list(inner, element, map, prefixes, raw)?,
                ListItem::Item(text) => Value::Term(match raw {
                    Raw::Term(_) => to_term(text, element, map, prefixes)?,
                    _ => literal_term(text, element, map, prefixes)?,
                }),
            });
        }
        Ok(Value::List(out))
    }
}

/// A term's text: a literal's lexical form, an IRI, a blank node's label.
fn text_of(t: &Term) -> String {
    match t {
        Term::Literal(l) => l.value().to_owned(),
        Term::NamedNode(n) => n.as_str().to_owned(),
        Term::BlankNode(b) => b.as_str().to_owned(),
        #[allow(unreachable_patterns)]
        other => other.to_string(),
    }
}

/// The type a term has of itself: `ottr:IRI` for an IRI, its datatype for a literal.
fn intrinsic(t: &Term) -> TypeRef {
    match t {
        Term::NamedNode(_) => basic(types::IRI),
        Term::Literal(l) => {
            let dt = tarka_ottr::lint::literal_type(l);
            match &dt {
                TypeRef::Basic(n) if types::is_known(n.as_str()) => dt,
                _ => basic(LITERAL),
            }
        }
        _ => basic(RESOURCE),
    }
}

/// The innermost element type of a list type.
fn list_element(t: &TypeRef) -> Option<TypeRef> {
    match t {
        TypeRef::List(i) | TypeRef::NeList(i) => Some(list_element(i).unwrap_or_else(|| without_lub(i))),
        TypeRef::Lub(i) => list_element(i),
        TypeRef::Basic(_) => None,
    }
}

fn without_lub(t: &TypeRef) -> TypeRef {
    match t {
        TypeRef::Lub(i) => without_lub(i),
        other => other.clone(),
    }
}

/// A literal from text: tagged or typed by the map's settings, else by the type.
fn literal_term(text: &str, ty: &TypeRef, map: &ArgumentMap, prefixes: &PrefixMap) -> Result<Term, String> {
    if let Some(tag) = &map.language_tag {
        return Ok(Literal::new_language_tagged_literal_unchecked(text, tag.to_ascii_lowercase()).into());
    }
    if let Some(sep) = &map.language_tag_sep {
        let (value, tag) =
            text.rsplit_once(sep.as_str()).ok_or_else(|| format!("value '{text}' does not contain language tag separator '{sep}'"))?;
        return Literal::new_language_tagged_literal(value, tag).map(Into::into).map_err(|_| format!("'{tag}' is not a language tag"));
    }
    if let Some(sep) = &map.datatype_sep {
        let (value, dt) =
            text.rsplit_once(sep.as_str()).ok_or_else(|| format!("value '{text}' does not contain datatype separator '{sep}'"))?;
        let dt = NamedNode::new(expand(dt, prefixes)).map_err(|_| format!("'{dt}' is not a datatype IRI"))?;
        return typed(value, &dt);
    }
    to_term(text, ty, map, prefixes)
}

/// A term from text, by type.
fn to_term(text: &str, ty: &TypeRef, map: &ArgumentMap, prefixes: &PrefixMap) -> Result<Term, String> {
    if types::subtype(ty, &basic(types::IRI)) && !matches!(ty, TypeRef::Basic(n) if n.as_str() == types::BOT) {
        let iri = expand(text, prefixes);
        return NamedNode::new(&iri).map(Into::into).map_err(|_| format!("'{text}' is not an IRI"));
    }
    let literal = basic(LITERAL);
    if let TypeRef::Basic(dt) = ty
        && types::subtype(ty, &literal)
        && *ty != literal
    {
        if dt.as_ref() == xsd::BOOLEAN {
            if map.boolean_true.iter().any(|t| t == text) {
                return Ok(Literal::from(true).into());
            }
            if map.boolean_false.iter().any(|f| f == text) {
                return Ok(Literal::from(false).into());
            }
        }
        return typed(text, dt);
    }
    Ok(Literal::new_simple_literal(text).into())
}

/// A typed literal, as written, if `text` is in the datatype's lexical space.
fn typed(text: &str, dt: &NamedNode) -> Result<Term, String> {
    if dt.as_ref() == xsd::STRING {
        return Ok(Literal::new_simple_literal(text).into());
    }
    if dt.as_ref() == rdf::LANG_STRING {
        return Err(format!("'{text}' has no language tag, so it cannot be an rdf:langString"));
    }
    if !valid(dt.as_str(), text) {
        return Err(format!("the value '{text}' is not in the lexical space of its datatype {}", dt.as_str()));
    }
    Ok(Literal::new_typed_literal(text, dt.clone()).into())
}

/// A prefixed name expanded with `prefixes`; anything else as it is.
fn expand(text: &str, prefixes: &PrefixMap) -> String {
    match text.split_once(':') {
        Some((p, local)) if !local.starts_with("//") => match prefixes.get(p) {
            Some(ns) => format!("{ns}{local}"),
            None => text.to_owned(),
        },
        _ => text.to_owned(),
    }
}

/// Whether `text` is a value of an XSD datatype (true for datatypes tarka does not know).
pub fn valid(datatype: &str, text: &str) -> bool {
    use oxsdatatypes::*;
    let Some(local) = datatype.strip_prefix("http://www.w3.org/2001/XMLSchema#") else { return true };
    let int = |min: i128, max: i128| Integer::from_str(text).is_ok() && text.trim().parse::<i128>().is_ok_and(|v| v >= min && v <= max);
    match local {
        "boolean" => Boolean::from_str(text).is_ok(),
        "decimal" => Decimal::from_str(text).is_ok(),
        "integer" => Integer::from_str(text).is_ok(),
        "long" => int(i64::MIN as i128, i64::MAX as i128),
        "int" => int(i32::MIN as i128, i32::MAX as i128),
        "short" => int(i16::MIN as i128, i16::MAX as i128),
        "byte" => int(i8::MIN as i128, i8::MAX as i128),
        "nonNegativeInteger" => int(0, i128::MAX),
        "positiveInteger" => int(1, i128::MAX),
        "nonPositiveInteger" => int(i128::MIN, 0),
        "negativeInteger" => int(i128::MIN, -1),
        "unsignedLong" => int(0, u64::MAX as i128),
        "unsignedInt" => int(0, u32::MAX as i128),
        "unsignedShort" => int(0, u16::MAX as i128),
        "unsignedByte" => int(0, u8::MAX as i128),
        "double" => Double::from_str(text).is_ok(),
        "float" => Float::from_str(text).is_ok(),
        "dateTime" => DateTime::from_str(text).is_ok(),
        "dateTimeStamp" => DateTime::from_str(text).is_ok_and(|d| d.timezone_offset().is_some()),
        "date" => Date::from_str(text).is_ok(),
        "time" => Time::from_str(text).is_ok(),
        "duration" => Duration::from_str(text).is_ok(),
        "dayTimeDuration" => DayTimeDuration::from_str(text).is_ok(),
        "yearMonthDuration" => YearMonthDuration::from_str(text).is_ok(),
        "gYear" => GYear::from_str(text).is_ok(),
        "gYearMonth" => GYearMonth::from_str(text).is_ok(),
        "gMonth" => GMonth::from_str(text).is_ok(),
        "gDay" => GDay::from_str(text).is_ok(),
        "gMonthDay" => GMonthDay::from_str(text).is_ok(),
        _ => true,
    }
}

/// An item of a list read from text: a value, or a nested list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListItem {
    Item(String),
    List(Vec<ListItem>),
}

/// Reads a list from text as Lutra does: `(a, b, (c, d))`, items trimmed; text without
/// the brackets is one list.
pub fn parse_list(text: &str, start: char, end: char, sep: &str) -> Result<Vec<ListItem>, String> {
    let wrapped;
    let text = if !text.contains(start) && !text.contains(end) {
        wrapped = format!("{start}{text}{end}");
        wrapped.as_str()
    } else {
        text
    };
    let mut stack: Vec<Vec<ListItem>> = Vec::new();
    let mut result = None;
    let mut since = 0;
    let mut prev_end = false;
    for (i, c) in text.char_indices() {
        if c != start && c != end {
            continue;
        }
        let chunk = text[since..i].trim();
        if !chunk.is_empty() {
            let Some(top) = stack.last_mut() else { return Err(format!("unbalanced lists: \"dangling\" list content: {chunk}")) };
            let mut parts: Vec<&str> = chunk.split(sep).map(str::trim).collect();
            // the separator after a nested list, and the one before one
            if prev_end && parts.first() == Some(&"") {
                parts.remove(0);
            }
            if c == start && parts.last() == Some(&"") {
                parts.pop();
            }
            top.extend(parts.into_iter().map(|p| ListItem::Item(p.to_owned())));
        }
        if c == start {
            stack.push(Vec::new());
            prev_end = false;
        } else {
            let done = stack.pop().ok_or_else(|| format!("unbalanced lists, more '{end}' than '{start}'"))?;
            match stack.last_mut() {
                Some(parent) => parent.push(ListItem::List(done)),
                None if result.is_none() => result = Some(done),
                None => return Err(format!("could not read a list from: {text}")),
            }
            prev_end = true;
        }
        since = i + c.len_utf8();
    }
    if !stack.is_empty() {
        return Err(format!("unbalanced lists, more '{start}' than '{end}'"));
    }
    if !text[since..].trim().is_empty() {
        return Err(format!("unbalanced lists: \"dangling\" list content: {}", text[since..].trim()));
    }
    result.ok_or_else(|| format!("could not read a list from: {text}"))
}

/// The type of an argument.
fn value_type(v: &Value) -> TypeRef {
    match v {
        Value::Term(Term::NamedNode(_)) => TypeRef::Lub(Box::new(basic(types::IRI))),
        Value::Term(Term::Literal(l)) => tarka_ottr::lint::literal_type(l),
        Value::Term(_) => TypeRef::Lub(Box::new(basic(RESOURCE))),
        Value::List(items) => match items.iter().map(value_type).reduce(|a, b| types::join(&a, &b)) {
            Some(e) => TypeRef::NeList(Box::new(e)),
            None => TypeRef::List(Box::new(basic(types::BOT))),
        },
    }
}

fn show(v: &Value, prefixes: &PrefixMap) -> String {
    match v {
        Value::Term(Term::NamedNode(n)) => prefixes.compact(n.as_str()).unwrap_or_else(|| n.to_string()),
        Value::Term(t) => t.to_string(),
        Value::List(items) => format!("({})", items.iter().map(|i| show(i, prefixes)).collect::<Vec<_>>().join(", ")),
    }
}

/// Fails unless every argument fits its parameter (as Lutra checks an instance).
pub fn check(args: &[Option<Value>], params: &[Param], prefixes: &PrefixMap) -> Result<(), String> {
    fn fits(v: &Value, t: &TypeRef) -> bool {
        match (v, t) {
            (Value::List(items), TypeRef::List(e) | TypeRef::NeList(e)) => {
                !(items.is_empty() && matches!(t, TypeRef::NeList(_))) && items.iter().all(|i| fits(i, e))
            }
            _ => compatible(&value_type(v), t),
        }
    }
    for (i, (arg, param)) in args.iter().zip(params).enumerate() {
        let Some(v) = arg else { continue };
        let t = effective(param.ty.as_ref());
        if !fits(v, &t) {
            return Err(format!(
                "argument {} {} ({}) does not fit ?{} : {}",
                i + 1,
                show(v, prefixes),
                display(&value_type(v), prefixes),
                param.name,
                display(&t, prefixes)
            ));
        }
        if param.non_blank && matches!(v, Value::Term(Term::BlankNode(_))) {
            return Err(format!("argument {} is a blank node, and ?{} is non-blank", i + 1, param.name));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> ListItem {
        ListItem::Item(v.into())
    }

    /// The lists Lutra read from the same text.
    #[test]
    fn lists() {
        let read = |t: &str| parse_list(t, '(', ')', ",");
        assert_eq!(read("(x,y,z)").unwrap(), [s("x"), s("y"), s("z")]);
        assert_eq!(read("(w)").unwrap(), [s("w")]);
        assert_eq!(read("()").unwrap(), []);
        assert_eq!(read("((n1,n2),m)").unwrap(), [ListItem::List(vec![s("n1"), s("n2")]), s("m")]);
        assert_eq!(read("p, q").unwrap(), [s("p"), s("q")]);
        assert_eq!(read("( s , ,t )").unwrap(), [s("s"), s(""), s("t")]);
        assert_eq!(read("(a,(b),c)").unwrap(), [s("a"), ListItem::List(vec![s("b")]), s("c")]);
        assert!(read("(a").is_err());
        assert!(read("a)").is_err());
        assert!(read("(a)b").is_err());
        assert_eq!(parse_list("[x|y]", '[', ']', "|").unwrap(), [s("x"), s("y")], "a separator is text, not a pattern");
    }

    #[test]
    fn arguments() {
        let prefixes: PrefixMap = [("ex", "http://example.com/ns#"), ("xsd", "http://www.w3.org/2001/XMLSchema#")].into_iter().collect();
        let mut c = Converter::default();
        let text = |t: &str| Raw::Text(t.into());
        let ty = |iri: &str| ArgumentMap { ty: Some(basic(iri)), ..ArgumentMap::default() };
        let term = |v: Option<Value>| match v {
            Some(Value::Term(t)) => t.to_string(),
            other => format!("{other:?}"),
        };
        let iri = ty(types::IRI);
        assert_eq!(term(c.argument(&text("ex:a"), &iri, &prefixes).unwrap()), "<http://example.com/ns#a>");
        assert_eq!(term(c.argument(&text("foo:bar"), &iri, &prefixes).unwrap()), "<foo:bar>", "an undeclared prefix is a scheme");
        assert!(c.argument(&text("Q"), &iri, &prefixes).is_err());
        let int = ty("http://www.w3.org/2001/XMLSchema#integer");
        assert_eq!(term(c.argument(&text("007"), &int, &prefixes).unwrap()), "\"007\"^^<http://www.w3.org/2001/XMLSchema#integer>");
        assert!(c.argument(&text("x"), &int, &prefixes).is_err());
        assert_eq!(term(c.argument(&text("hi"), &ArgumentMap::default(), &prefixes).unwrap()), "\"hi\"");
        assert_eq!(c.argument(&Raw::Null, &ArgumentMap::default(), &prefixes).unwrap(), None, "ottr:none");
        let tagged = ArgumentMap { language_tag_sep: Some("@".into()), ..ArgumentMap::default() };
        assert_eq!(term(c.argument(&text("hello@en"), &tagged, &prefixes).unwrap()), "\"hello\"@en");
        assert!(c.argument(&text("plain"), &tagged, &prefixes).is_err());
        let dt = ArgumentMap { datatype_sep: Some("^^".into()), ..ArgumentMap::default() };
        assert_eq!(term(c.argument(&text("42^^xsd:int"), &dt, &prefixes).unwrap()), "\"42\"^^<http://www.w3.org/2001/XMLSchema#int>");
        // blank nodes: one per label, a new one for a fresh value
        let blanks = ArgumentMap { fresh_blank: vec!["new".into()], ..ArgumentMap::default() };
        let a = term(c.argument(&text("_:x"), &blanks, &prefixes).unwrap());
        assert_eq!(a, term(c.argument(&text("_:x"), &blanks, &prefixes).unwrap()));
        assert_ne!(
            term(c.argument(&text("new"), &blanks, &prefixes).unwrap()),
            term(c.argument(&text("new"), &blanks, &prefixes).unwrap())
        );
        // an RDF term that fits is kept; one that does not is read again
        let lit = |v: &str, l: Option<&str>| {
            Raw::Term(match l {
                Some(l) => Literal::new_language_tagged_literal_unchecked(v, l).into(),
                None => Literal::new_typed_literal(v, xsd::INTEGER).into(),
            })
        };
        assert_eq!(
            term(c.argument(&lit("42", None), &ty("http://www.w3.org/2001/XMLSchema#decimal"), &prefixes).unwrap()),
            "\"42\"^^<http://www.w3.org/2001/XMLSchema#integer>"
        );
        assert_eq!(
            term(c.argument(&lit("Bob", Some("en")), &ty("http://www.w3.org/2001/XMLSchema#string"), &prefixes).unwrap()),
            "\"Bob\""
        );
        assert!(valid("http://www.w3.org/2001/XMLSchema#byte", "127") && !valid("http://www.w3.org/2001/XMLSchema#byte", "128"));
    }
}
