//! What kind of RDF term each of a plan's variables holds: from the OTTR types its
//! columns are converted by, or inferred from the expressions a TARQL query binds.

use std::collections::{BTreeSet, HashMap};

use oxrdf::vocab::{rdf, xsd};
use oxrdf::{NamedNode, Term};
use spargebra::algebra::{Expression, Function, GraphPattern};
use spargebra::term::GroundTerm;
use tarka_core::{BNodeId, Block, CellSource, Conversion, Lifting, Plan, SparqlLifting, TermPat, VarId};

/// One kind of term a pattern can give.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Iri,
    BlankNode,
    /// A literal of this datatype (`rdf:langString` for a language-tagged string).
    Literal(NamedNode),
    /// A literal of a datatype not known here.
    AnyLiteral,
    /// An RDF collection: its head blank node, or `rdf:nil`.
    List,
}

/// The kinds a pattern can give, or `None` when that is not known.
pub type Kinds = Option<BTreeSet<Kind>>;

fn one(k: Kind) -> Kinds {
    Some(BTreeSet::from([k]))
}

fn literal(dt: NamedNode) -> Kinds {
    one(Kind::Literal(dt))
}

fn string() -> Kinds {
    literal(xsd::STRING.into())
}

/// The kinds either pattern can give.
pub fn join(a: Kinds, b: Kinds) -> Kinds {
    let (mut a, b) = (a?, b?);
    a.extend(b);
    Some(a)
}

/// The kinds of a plan's variables, by `VarId`.
pub fn var_kinds(plan: &Plan) -> Vec<Kinds> {
    let mut kinds: Vec<Kinds> = vec![None; plan.vars.len()];
    // the kinds of a list variable's elements
    let mut elements: HashMap<VarId, Kinds> = HashMap::new();
    match &plan.lifting {
        Lifting::Given => {}
        Lifting::Columns(bindings) => {
            for b in bindings {
                let k = match &b.source {
                    // the engine binds the row number as an integer
                    CellSource::RowNumber => literal(xsd::INTEGER.into()),
                    CellSource::Column(_) => conversion(&b.conversion),
                };
                if b.list {
                    kinds[b.var.0] = one(Kind::List);
                    elements.insert(b.var, k);
                } else {
                    kinds[b.var.0] = k;
                }
            }
        }
        Lifting::Sparql(lifting) => {
            let inference = Inference::new(lifting);
            for (var, v) in &lifting.outputs {
                kinds[var.0] = inference.variable(v.as_str(), &mut Vec::new());
            }
        }
    }
    // the element variables of repeats take the kinds of their lists' elements
    fn repeats(block: &Block, elements: &HashMap<VarId, Kinds>, kinds: &mut [Kinds]) {
        for r in &block.repeats {
            for (list, element) in &r.lists {
                kinds[element.0] = elements.get(list).cloned().flatten();
            }
            repeats(&r.body, elements, kinds);
        }
    }
    repeats(&plan.root, &elements, &mut kinds);
    kinds
}

fn conversion(c: &Conversion) -> Kinds {
    match c {
        Conversion::Plain => string(),
        Conversion::Iri => one(Kind::Iri),
        Conversion::Typed(dt) => literal(dt.clone()),
        Conversion::Lang(_) => literal(rdf::LANG_STRING.into()),
    }
}

/// The kinds of a term pattern, given its variables' kinds.
pub fn term_kinds(t: &TermPat, vars: &[Kinds]) -> Kinds {
    match t {
        TermPat::Const(term) => term_kind(term),
        TermPat::Var(v) => vars[v.0].clone(),
        TermPat::BNode(_) => one(Kind::BlankNode),
        TermPat::Default(v, fallback) => join(vars[v.0].clone(), term_kinds(fallback, vars)),
        TermPat::List(items) if items.is_empty() => one(Kind::Iri),
        TermPat::List(_) => one(Kind::BlankNode),
    }
}

fn term_kind(term: &Term) -> Kinds {
    match term {
        Term::NamedNode(_) => one(Kind::Iri),
        Term::BlankNode(_) => one(Kind::BlankNode),
        Term::Literal(l) => literal(l.datatype().into_owned()),
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

/// The blank node a term pattern always is, if any.
pub fn bnode(t: &TermPat) -> Option<BNodeId> {
    match t {
        TermPat::BNode(b) => Some(*b),
        _ => None,
    }
}

/// Type inference over a TARQL WHERE clause.
struct Inference<'a> {
    /// The expressions bound to each variable (several in different branches).
    binds: HashMap<&'a str, Vec<&'a Expression>>,
    /// The terms a VALUES clause gives each variable (None for UNDEF).
    values: HashMap<&'a str, Vec<Option<&'a GroundTerm>>>,
    /// The row's columns: plain strings.
    columns: &'a [String],
}

impl<'a> Inference<'a> {
    fn new(lifting: &'a SparqlLifting) -> Self {
        let mut this = Self { binds: HashMap::new(), values: HashMap::new(), columns: &lifting.variables };
        this.walk(&lifting.pattern);
        this
    }

    fn walk(&mut self, p: &'a GraphPattern) {
        match p {
            GraphPattern::Extend { inner, variable, expression } => {
                self.binds.entry(variable.as_str()).or_default().push(expression);
                self.walk(inner);
            }
            GraphPattern::Values { variables, bindings } => {
                for (i, v) in variables.iter().enumerate() {
                    self.values.entry(v.as_str()).or_default().extend(bindings.iter().map(|row| row[i].as_ref()));
                }
            }
            GraphPattern::Join { left, right } | GraphPattern::LeftJoin { left, right, .. } | GraphPattern::Union { left, right } => {
                self.walk(left);
                self.walk(right);
            }
            GraphPattern::Minus { left, .. } => self.walk(left),
            GraphPattern::Filter { inner, .. }
            | GraphPattern::Graph { inner, .. }
            | GraphPattern::OrderBy { inner, .. }
            | GraphPattern::Project { inner, .. }
            | GraphPattern::Distinct { inner }
            | GraphPattern::Reduced { inner }
            | GraphPattern::Slice { inner, .. } => self.walk(inner),
            // triple patterns, paths, aggregates and services bind nothing known here
            _ => {}
        }
    }

    /// The kinds of a variable; `seen` stops cycles through BINDs.
    fn variable(&self, name: &str, seen: &mut Vec<String>) -> Kinds {
        if seen.iter().any(|s| s == name) {
            return None;
        }
        if let Some(exprs) = self.binds.get(name) {
            seen.push(name.to_owned());
            let mut out = Some(BTreeSet::new());
            for e in exprs {
                out = join(out, self.expression(e, seen));
            }
            seen.pop();
            return out;
        }
        if let Some(terms) = self.values.get(name) {
            let mut out = Some(BTreeSet::new());
            for t in terms.iter().flatten() {
                out = join(out, ground(t));
            }
            return out;
        }
        if name == "ROWNUM" {
            return literal(xsd::INTEGER.into());
        }
        if self.columns.iter().any(|c| c == name) {
            // a row binds its cells as plain strings
            return string();
        }
        None
    }

    fn expression(&self, e: &Expression, seen: &mut Vec<String>) -> Kinds {
        let boolean = || literal(xsd::BOOLEAN.into());
        match e {
            Expression::NamedNode(_) => one(Kind::Iri),
            Expression::Literal(l) => literal(l.datatype().into_owned()),
            Expression::Variable(v) => self.variable(v.as_str(), seen),
            Expression::Or(..)
            | Expression::And(..)
            | Expression::Equal(..)
            | Expression::SameTerm(..)
            | Expression::Greater(..)
            | Expression::GreaterOrEqual(..)
            | Expression::Less(..)
            | Expression::LessOrEqual(..)
            | Expression::In(..)
            | Expression::Not(_)
            | Expression::Exists(_)
            | Expression::Bound(_) => boolean(),
            Expression::Add(..)
            | Expression::Subtract(..)
            | Expression::Multiply(..)
            | Expression::Divide(..)
            | Expression::UnaryPlus(_)
            | Expression::UnaryMinus(_) => one(Kind::AnyLiteral),
            Expression::If(_, then, otherwise) => join(self.expression(then, seen), self.expression(otherwise, seen)),
            Expression::Coalesce(items) => {
                let mut out = Some(BTreeSet::new());
                for item in items {
                    out = join(out, self.expression(item, seen));
                }
                out
            }
            Expression::FunctionCall(f, args) => self.function(f, args, seen),
        }
    }

    fn function(&self, f: &Function, args: &[Expression], seen: &mut Vec<String>) -> Kinds {
        let boolean = || literal(xsd::BOOLEAN.into());
        let integer = || literal(xsd::INTEGER.into());
        // string functions keep a language tag, so they give xsd:string for strings only
        let stringy = |args: &[Expression], seen: &mut Vec<String>| {
            if args.iter().all(|a| self.expression(a, seen) == string()) { string() } else { one(Kind::AnyLiteral) }
        };
        match f {
            Function::Str
            | Function::Lang
            | Function::EncodeForUri
            | Function::Tz
            | Function::StrUuid
            | Function::Md5
            | Function::Sha1
            | Function::Sha256
            | Function::Sha384
            | Function::Sha512 => string(),
            Function::LCase | Function::UCase | Function::Replace => stringy(&args[..1], seen),
            Function::SubStr | Function::StrBefore | Function::StrAfter => stringy(&args[..1], seen),
            Function::Concat => stringy(args, seen),
            Function::StrLen | Function::Year | Function::Month | Function::Day | Function::Hours | Function::Minutes => integer(),
            Function::Seconds => literal(xsd::DECIMAL.into()),
            Function::Timezone => literal(xsd::DAY_TIME_DURATION.into()),
            Function::Now => literal(xsd::DATE_TIME.into()),
            Function::Rand => literal(xsd::DOUBLE.into()),
            Function::Abs | Function::Ceil | Function::Floor | Function::Round => {
                args.first().and_then(|a| self.expression(a, seen)).or_else(|| one(Kind::AnyLiteral))
            }
            Function::LangMatches
            | Function::Contains
            | Function::StrStarts
            | Function::StrEnds
            | Function::Regex
            | Function::IsIri
            | Function::IsBlank
            | Function::IsLiteral
            | Function::IsNumeric => boolean(),
            Function::Iri | Function::Datatype | Function::Uuid => one(Kind::Iri),
            Function::BNode => one(Kind::BlankNode),
            Function::StrLang => literal(rdf::LANG_STRING.into()),
            Function::StrDt => match args.get(1) {
                Some(Expression::NamedNode(dt)) => literal(evaluated_datatype(dt)),
                _ => one(Kind::AnyLiteral),
            },
            Function::Custom(iri) => custom(iri),
            #[allow(unreachable_patterns)]
            _ => None,
        }
    }
}

/// A custom function: an XSD cast, or one of TARQL's.
fn custom(iri: &NamedNode) -> Kinds {
    let name = iri.as_str();
    if name.starts_with(XSD) {
        return literal(evaluated_datatype(iri));
    }
    let local = tarka_tarql::TARQL_NAMESPACES.iter().find_map(|ns| name.strip_prefix(ns));
    match local {
        Some("expandPrefixedName") => one(Kind::Iri),
        Some("expandPrefix") => string(),
        _ => None,
    }
}

const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

/// The datatype spareval gives a value cast or typed as `dt`: types derived from
/// `xsd:integer` become `xsd:integer`, and `xsd:dateTimeStamp` `xsd:dateTime`.
fn evaluated_datatype(dt: &NamedNode) -> NamedNode {
    const INTEGERS: [&str; 12] = [
        "long",
        "int",
        "short",
        "byte",
        "nonNegativeInteger",
        "positiveInteger",
        "nonPositiveInteger",
        "negativeInteger",
        "unsignedLong",
        "unsignedInt",
        "unsignedShort",
        "unsignedByte",
    ];
    match dt.as_str().strip_prefix(XSD) {
        Some(local) if INTEGERS.contains(&local) => xsd::INTEGER.into(),
        Some("dateTimeStamp") => xsd::DATE_TIME.into(),
        _ => dt.clone(),
    }
}

fn ground(t: &GroundTerm) -> Kinds {
    match t {
        GroundTerm::NamedNode(_) => one(Kind::Iri),
        GroundTerm::Literal(l) => literal(l.datatype().into_owned()),
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds_of(query: &str) -> HashMap<String, Kinds> {
        let plan = tarka_tarql::parse_tarql(query, "q").unwrap();
        let kinds = var_kinds(&plan);
        plan.vars.iter().enumerate().map(|(i, v)| (v.name.clone(), kinds[i].clone())).collect()
    }

    #[test]
    fn tarql_binds() {
        let k = kinds_of(
            "PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>
             CONSTRUCT { ?s ?p ?o . ?s ?p ?n . ?s ?p ?d . ?s ?p ?c . ?s ?p ?l . ?s ?p ?x . ?s ?p ?b . ?s ?p ?u . ?s ?p ?row . ?s ?p ?i }
             WHERE {
               BIND(IRI(CONCAT(\"http://example.com/\", ?id)) AS ?s)
               BIND(tarql:expandPrefixedName(?kind) AS ?p)
               BIND(xsd:integer(?age) AS ?n)
               BIND(STRDT(?when, xsd:date) AS ?d)
               BIND(COALESCE(xsd:decimal(?price), 0.0) AS ?c)
               BIND(STRLANG(?label, \"en\") AS ?l)
               BIND(IF(BOUND(?age), ?n, \"none\") AS ?x)
               BIND(BNODE() AS ?b)
               BIND(UCASE(?name) AS ?u)
               BIND(STRDT(?count, xsd:int) AS ?i)
               BIND(?ROWNUM AS ?row)
             }",
        );
        let lit = |dt: NamedNodeRef<'_>| literal(dt.into());
        assert_eq!(k["s"], one(Kind::Iri));
        assert_eq!(k["p"], one(Kind::Iri));
        assert_eq!(k["o"], lit(xsd::STRING), "not bound by the query, so a column");
        assert_eq!(k["n"], lit(xsd::INTEGER));
        assert_eq!(k["d"], lit(xsd::DATE));
        assert_eq!(k["c"], lit(xsd::DECIMAL));
        assert_eq!(k["l"], lit(rdf::LANG_STRING));
        assert_eq!(k["x"], join(lit(xsd::INTEGER), lit(xsd::STRING)));
        assert_eq!(k["b"], one(Kind::BlankNode));
        assert_eq!(k["u"], lit(xsd::STRING), "a column is a plain string");
        assert_eq!(k["i"], lit(xsd::INTEGER), "spareval writes xsd:int values as xsd:integer");
        assert_eq!(k["row"], lit(xsd::INTEGER));
    }

    use oxrdf::NamedNodeRef;
}
