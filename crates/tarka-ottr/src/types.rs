//! OTTR's type system: the hierarchy of basic types, list types and LUB types, and
//! when an argument fits a parameter. The rules follow Lutra's type checker.
//!
//! * Basic types form a tree under `rdfs:Resource`; `ottr:Bot` is below everything.
//!   A type the hierarchy does not name is read as `rdfs:Resource`, as Lutra reads it.
//! * `NEList<T>` is a subtype of `List<T>`, and lists are covariant in their element
//!   type. Every type, lists included, is a subtype of `rdfs:Resource`.
//! * A term's type is exact for a literal (its datatype; `xsd:string` for a plain
//!   literal, `rdf:langString` for a tagged one) and a least upper bound for an IRI
//!   (`LUB<ottr:IRI>`) or a blank node (`LUB<rdfs:Resource>`): an IRI fits any IRI type,
//!   `owl:Class` included, since nothing says which it is.
//! * An argument fits a parameter when its type is a subtype of the parameter's, or,
//!   for `LUB<T>`, when the parameter's type is a subtype of T (or T of it).

use std::collections::HashMap;
use std::sync::OnceLock;

use oxrdf::NamedNode;
use tarka_core::{PrefixMap, TypeRef};

const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const OWL: &str = "http://www.w3.org/2002/07/owl#";
const OTTR: &str = "http://ns.ottr.xyz/0.4/";

pub const RESOURCE: &str = "http://www.w3.org/2000/01/rdf-schema#Resource";
pub const BOT: &str = "http://ns.ottr.xyz/0.4/Bot";
pub const IRI: &str = "http://ns.ottr.xyz/0.4/IRI";
pub const LITERAL: &str = "http://www.w3.org/2000/01/rdf-schema#Literal";

/// Each basic type and its supertype.
const HIERARCHY: &[(&str, &str, &str, &str)] = &[
    (OTTR, "IRI", RDFS, "Resource"),
    (RDFS, "Literal", RDFS, "Resource"),
    (OTTR, "string", RDFS, "Literal"),
    (XSD, "string", OTTR, "string"),
    (RDF, "langString", OTTR, "string"),
    (XSD, "normalizedString", XSD, "string"),
    (XSD, "token", XSD, "normalizedString"),
    (XSD, "language", XSD, "token"),
    (XSD, "Name", XSD, "token"),
    (XSD, "NCName", XSD, "Name"),
    (XSD, "NMTOKEN", XSD, "Name"),
    (OWL, "real", RDFS, "Literal"),
    (OWL, "rational", OWL, "real"),
    (XSD, "decimal", OWL, "rational"),
    (XSD, "integer", XSD, "decimal"),
    (XSD, "long", XSD, "integer"),
    (XSD, "int", XSD, "long"),
    (XSD, "short", XSD, "int"),
    (XSD, "byte", XSD, "short"),
    (XSD, "nonNegativeInteger", XSD, "integer"),
    (XSD, "positiveInteger", XSD, "nonNegativeInteger"),
    (XSD, "unsignedLong", XSD, "positiveInteger"),
    (XSD, "unsignedInt", XSD, "unsignedLong"),
    (XSD, "unsignedShort", XSD, "unsignedInt"),
    (XSD, "unsignedByte", XSD, "unsignedShort"),
    (XSD, "nonPositiveInteger", XSD, "integer"),
    (XSD, "negativeInteger", XSD, "nonPositiveInteger"),
    (XSD, "double", RDFS, "Literal"),
    (XSD, "float", RDFS, "Literal"),
    (XSD, "boolean", RDFS, "Literal"),
    (XSD, "dateTime", RDFS, "Literal"),
    (XSD, "dateTimeStamp", XSD, "dateTime"),
    (XSD, "date", RDFS, "Literal"),
    (XSD, "time", RDFS, "Literal"),
    (XSD, "duration", RDFS, "Literal"),
    (XSD, "dayTimeDuration", XSD, "duration"),
    (XSD, "yearMonthDuration", XSD, "duration"),
    (XSD, "gDay", RDFS, "Literal"),
    (XSD, "gMonth", RDFS, "Literal"),
    (XSD, "gMonthDay", RDFS, "Literal"),
    (XSD, "gYear", RDFS, "Literal"),
    (XSD, "gYearMonth", RDFS, "Literal"),
    (XSD, "hexBinary", RDFS, "Literal"),
    (XSD, "base64Binary", RDFS, "Literal"),
    (XSD, "anyURI", RDFS, "Literal"),
    (RDF, "HTML", RDFS, "Literal"),
    (RDF, "XMLLiteral", RDFS, "Literal"),
    (OWL, "Class", OTTR, "IRI"),
    (OWL, "NamedIndividual", OTTR, "IRI"),
    (OWL, "ObjectProperty", OTTR, "IRI"),
    (OWL, "DatatypeProperty", OTTR, "IRI"),
    (OWL, "AnnotationProperty", OTTR, "IRI"),
    (RDFS, "Datatype", OTTR, "IRI"),
];

fn parents() -> &'static HashMap<String, String> {
    static PARENTS: OnceLock<HashMap<String, String>> = OnceLock::new();
    PARENTS.get_or_init(|| HIERARCHY.iter().map(|(ns, l, pns, pl)| (format!("{ns}{l}"), format!("{pns}{pl}"))).collect())
}

/// Whether the hierarchy names this basic type.
pub fn is_known(iri: &str) -> bool {
    iri == RESOURCE || iri == BOT || parents().contains_key(iri)
}

/// A basic type by IRI.
pub fn basic(iri: &str) -> TypeRef {
    TypeRef::Basic(NamedNode::new_unchecked(iri))
}

/// A parameter's type as the checker reads it: untyped is `rdfs:Resource`, and so is a
/// basic type the hierarchy does not name.
pub fn effective(t: Option<&TypeRef>) -> TypeRef {
    fn known(t: &TypeRef) -> TypeRef {
        match t {
            TypeRef::Basic(n) if !is_known(n.as_str()) => basic(RESOURCE),
            TypeRef::Basic(_) => t.clone(),
            TypeRef::List(i) => TypeRef::List(Box::new(known(i))),
            TypeRef::NeList(i) => TypeRef::NeList(Box::new(known(i))),
            TypeRef::Lub(i) => TypeRef::Lub(Box::new(known(i))),
        }
    }
    t.map_or_else(|| basic(RESOURCE), known)
}

fn basic_subtype(a: &str, b: &str) -> bool {
    if a == BOT || b == RESOURCE {
        return true;
    }
    let mut t = a;
    loop {
        if t == b {
            return true;
        }
        match parents().get(t) {
            Some(p) => t = p,
            None => return false,
        }
    }
}

/// Whether `a` is a subtype of `b`.
pub fn subtype(a: &TypeRef, b: &TypeRef) -> bool {
    use TypeRef::*;
    match (a, b) {
        (Basic(x), _) if x.as_str() == BOT => true,
        (_, Basic(y)) if y.as_str() == RESOURCE => true,
        (Lub(x), _) => subtype(x, b),
        (_, Lub(y)) => subtype(a, y),
        (Basic(x), Basic(y)) => basic_subtype(x.as_str(), y.as_str()),
        (NeList(x), NeList(y) | List(y)) | (List(x), List(y)) => subtype(x, y),
        _ => false,
    }
}

/// Whether an argument of type `arg` fits a parameter of type `param`.
pub fn compatible(arg: &TypeRef, param: &TypeRef) -> bool {
    use TypeRef::*;
    match (arg, param) {
        (Lub(x), _) => subtype(param, x) || subtype(x, param),
        (NeList(x), NeList(y) | List(y)) | (List(x), List(y)) => compatible(x, y),
        _ => subtype(arg, param),
    }
}

/// Whether one argument can be given to parameters of both types: they have a common
/// subtype other than `ottr:Bot`.
pub fn consistent(a: &TypeRef, b: &TypeRef) -> bool {
    use TypeRef::*;
    match (a, b) {
        (Lub(x), _) => consistent(x, b),
        (_, Lub(y)) => consistent(a, y),
        (List(x) | NeList(x), List(y) | NeList(y)) => consistent(x, y),
        _ => subtype(a, b) || subtype(b, a),
    }
}

/// The least upper bound of two types: the nearest type both are subtypes of.
pub fn join(a: &TypeRef, b: &TypeRef) -> TypeRef {
    use TypeRef::*;
    if subtype(a, b) {
        return b.clone();
    }
    if subtype(b, a) {
        return a.clone();
    }
    match (a, b) {
        (Lub(x), Lub(y)) => Lub(Box::new(join(x, y))),
        (Lub(x), _) => join(x, b),
        (_, Lub(y)) => join(a, y),
        (NeList(x), NeList(y)) => NeList(Box::new(join(x, y))),
        (List(x) | NeList(x), List(y) | NeList(y)) => List(Box::new(join(x, y))),
        (Basic(x), Basic(y)) => {
            // the first ancestor of x that y is under
            let mut t = x.as_str();
            loop {
                if basic_subtype(y.as_str(), t) {
                    return basic(t);
                }
                match parents().get(t) {
                    Some(p) => t = p,
                    None => return basic(RESOURCE),
                }
            }
        }
        _ => basic(RESOURCE),
    }
}

/// A type as stOTTR writes it, with prefixed names where possible.
pub fn display(t: &TypeRef, prefixes: &PrefixMap) -> String {
    match t {
        TypeRef::Basic(n) => prefixes.compact(n.as_str()).unwrap_or_else(|| format!("<{}>", n.as_str())),
        TypeRef::List(i) => format!("List<{}>", display(i, prefixes)),
        TypeRef::NeList(i) => format!("NEList<{}>", display(i, prefixes)),
        TypeRef::Lub(i) => format!("LUB<{}>", display(i, prefixes)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> TypeRef {
        let pm: PrefixMap = [("xsd", XSD), ("rdf", RDF), ("rdfs", RDFS), ("owl", OWL), ("ottr", OTTR), ("schema", "https://schema.org/")]
            .into_iter()
            .collect();
        if let Some(inner) = s.strip_prefix("List<").and_then(|r| r.strip_suffix('>')) {
            return TypeRef::List(Box::new(t(inner)));
        }
        if let Some(inner) = s.strip_prefix("NEList<").and_then(|r| r.strip_suffix('>')) {
            return TypeRef::NeList(Box::new(t(inner)));
        }
        if let Some(inner) = s.strip_prefix("LUB<").and_then(|r| r.strip_suffix('>')) {
            return TypeRef::Lub(Box::new(t(inner)));
        }
        basic(&pm.expand(s).unwrap())
    }

    /// The cases Lutra's checker was run on (see the lint tests), and its answers.
    #[test]
    fn compatibility_as_lutra_decides_it() {
        let fits = [
            ("owl:Class", "ottr:IRI"),
            ("LUB<ottr:IRI>", "owl:Class"),
            ("LUB<ottr:IRI>", "ottr:IRI"),
            ("LUB<rdfs:Resource>", "xsd:string"),
            ("LUB<rdfs:Resource>", "List<ottr:IRI>"),
            ("xsd:integer", "xsd:decimal"),
            ("rdf:langString", "rdfs:Literal"),
            ("List<xsd:string>", "rdfs:Resource"),
            ("NEList<LUB<ottr:IRI>>", "List<ottr:IRI>"),
            ("List<owl:Class>", "List<ottr:IRI>"),
            ("ottr:Bot", "xsd:int"),
            ("xsd:boolean", "rdfs:Literal"),
        ];
        for (a, p) in fits {
            assert!(compatible(&t(a), &t(p)), "{a} should fit {p}");
        }
        let misfits = [
            ("rdfs:Resource", "ottr:IRI"),
            ("ottr:IRI", "owl:Class"),
            ("xsd:integer", "xsd:int"),
            ("xsd:integer", "xsd:string"),
            ("rdf:langString", "xsd:string"),
            ("xsd:string", "ottr:IRI"),
            ("LUB<ottr:IRI>", "xsd:decimal"),
            ("List<xsd:string>", "xsd:decimal"),
            ("List<xsd:string>", "NEList<xsd:string>"),
            ("NEList<xsd:string>", "List<ottr:IRI>"),
            ("List<owl:Class>", "List<xsd:int>"),
        ];
        for (a, p) in misfits {
            assert!(!compatible(&t(a), &t(p)), "{a} should not fit {p}");
        }
    }

    #[test]
    fn consistency_and_joins() {
        assert!(consistent(&t("ottr:IRI"), &t("owl:Class")));
        assert!(!consistent(&t("ottr:IRI"), &t("xsd:string")));
        assert!(!consistent(&t("xsd:int"), &t("xsd:string")));
        assert!(!consistent(&t("List<xsd:int>"), &t("List<ottr:IRI>")));
        assert_eq!(join(&t("xsd:int"), &t("xsd:nonNegativeInteger")), t("xsd:integer"));
        assert_eq!(join(&t("xsd:string"), &t("xsd:integer")), t("rdfs:Literal"));
        assert_eq!(join(&t("LUB<ottr:IRI>"), &t("LUB<ottr:IRI>")), t("LUB<ottr:IRI>"));
        assert_eq!(effective(Some(&t("schema:Thing"))), t("rdfs:Resource"), "an unknown type");
        assert_eq!(effective(None), t("rdfs:Resource"));
        let pm: PrefixMap = [("xsd", XSD)].into_iter().collect();
        assert_eq!(display(&t("NEList<xsd:string>"), &pm), "NEList<xsd:string>");
    }
}
