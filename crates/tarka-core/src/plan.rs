//! The mapping plan, which every front end compiles to and every backend runs.
//!
//! A plan has two layers:
//!
//! * **Lifting** turns one input row into values for the plan's variables: read
//!   columns and convert them by type (OTTR), or evaluate a SPARQL WHERE clause with
//!   the row bound (TARQL). One row can give several solutions.
//! * **Shape** turns each solution into triples. A [`Pattern`] is emitted when its
//!   terms are bound and every variable it `requires` is bound, which is how OTTR's
//!   mandatory parameters drop a whole template instance. A [`Block`] makes fresh
//!   blank nodes each time it runs, and a [`Repeat`] runs its body once per element
//!   of one or more list values (OTTR's `cross`, `zipMin` and `zipMax`).

use oxrdf::{Literal, NamedNode, Term, Variable};
use spargebra::algebra::GraphPattern;

use crate::literal::typed_literal;
use crate::prefix::PrefixMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VarId(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BNodeId(pub usize);

#[derive(Clone, Debug)]
pub struct Plan {
    /// What the plan was made from (a query file name or a template IRI).
    pub name: String,
    /// Prefixes for prefixed names in the data, and for Turtle output.
    pub prefixes: PrefixMap,
    pub vars: Vec<VarInfo>,
    pub bnode_count: usize,
    pub lifting: Lifting,
    pub root: Block,
    /// An input named by the mapping itself (TARQL's `FROM <file.csv>`).
    pub default_input: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct VarInfo {
    pub name: String,
    /// The declared type of every OTTR parameter the variable is passed to.
    pub types: Vec<TypeRef>,
    /// Passed to a non-blank (`!`) parameter.
    pub non_blank: bool,
}

/// An OTTR type: a basic type (its IRI), a list type or a least-upper-bound type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TypeRef {
    Basic(NamedNode),
    List(Box<TypeRef>),
    NeList(Box<TypeRef>),
    Lub(Box<TypeRef>),
}

impl TypeRef {
    /// The innermost basic type.
    pub fn basic(&self) -> &NamedNode {
        match self {
            Self::Basic(n) => n,
            Self::List(t) | Self::NeList(t) | Self::Lub(t) => t.basic(),
        }
    }

    pub fn is_list(&self) -> bool {
        matches!(self, Self::List(_) | Self::NeList(_))
    }
}

#[derive(Clone, Debug)]
pub enum Lifting {
    /// The values are given directly, as for ground OTTR instances.
    Given,
    /// Each variable is read from a column and converted by its type.
    Columns(Vec<ColumnBinding>),
    /// A SPARQL WHERE clause, evaluated with the row's cells bound (TARQL).
    Sparql(Box<SparqlLifting>),
}

#[derive(Clone, Debug)]
pub struct ColumnBinding {
    pub var: VarId,
    pub source: CellSource,
    pub conversion: Conversion,
    /// The parameter takes a list: a list cell gives its items, and a text cell is split
    /// on `list_separator`.
    pub list: bool,
    /// The separator of a text cell's list items.
    pub list_separator: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellSource {
    Column(String),
    /// The row number, from 0 (oxi-gen's `?ROWNUM`).
    RowNumber,
}

/// How a cell's text becomes a term.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Conversion {
    /// A plain string literal.
    Plain,
    /// A prefixed name with a declared prefix, or else a full IRI (see [`PrefixMap::resolve`]).
    Iri,
    /// A literal of this datatype, in canonical form when the text is a valid value.
    Typed(NamedNode),
    /// A language-tagged string.
    Lang(String),
}

impl Conversion {
    /// The term for `text`, or why it cannot become one (it is not an IRI, or the
    /// language tag is not one).
    pub fn apply(&self, text: &str, prefixes: &PrefixMap) -> Result<Term, String> {
        match self {
            Self::Plain => Ok(Literal::new_simple_literal(text).into()),
            Self::Iri => NamedNode::new(prefixes.resolve(text)).map(Into::into).map_err(|_| format!("'{text}' is not an IRI")),
            Self::Typed(dt) => Ok(typed_literal(text, dt.as_ref()).into()),
            Self::Lang(tag) => {
                Literal::new_language_tagged_literal(text, tag).map(Into::into).map_err(|_| format!("'{tag}' is not a language tag"))
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct SparqlLifting {
    /// The WHERE clause. Each row's cells are bound by a VALUES table placed first in
    /// the group, as TARQL does.
    pub pattern: GraphPattern,
    pub base_iri: Option<String>,
    /// Every variable the query mentions; row columns with these names are bound.
    pub variables: Vec<String>,
    /// Plan variable ← solution variable.
    pub outputs: Vec<(VarId, Variable)>,
    /// The WHERE clause has solution modifiers (`LIMIT`, `ORDER BY` …) at the top, so
    /// rows must be evaluated one at a time for them to apply per row.
    pub per_row: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Block {
    /// Blank nodes made fresh each time the block runs.
    pub bnodes: Vec<BNodeId>,
    pub patterns: Vec<Pattern>,
    pub repeats: Vec<Repeat>,
}

#[derive(Clone, Debug)]
pub struct Pattern {
    pub subject: TermPat,
    pub predicate: TermPat,
    pub object: TermPat,
    /// Variables that must be bound for the triple to be emitted, besides its own.
    pub requires: Vec<VarId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TermPat {
    Const(Term),
    Var(VarId),
    BNode(BNodeId),
    /// The variable's value, or the fallback when it is unbound (an OTTR default).
    Default(VarId, Box<TermPat>),
    /// An RDF collection of these items (emitted only if every item is bound).
    List(Vec<TermPat>),
}

#[derive(Clone, Debug)]
pub struct Repeat {
    pub expander: Expander,
    /// (list variable, element variable): each run binds the element variables.
    pub lists: Vec<(VarId, VarId)>,
    pub requires: Vec<VarId>,
    pub body: Block,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expander {
    Cross,
    ZipMin,
    ZipMax,
}

impl Plan {
    pub fn new(name: impl Into<String>, lifting: Lifting) -> Self {
        Self {
            name: name.into(),
            prefixes: PrefixMap::new(),
            vars: Vec::new(),
            bnode_count: 0,
            lifting,
            root: Block::default(),
            default_input: None,
        }
    }

    /// The variable named `name`, made if it does not exist yet.
    pub fn var(&mut self, name: &str) -> VarId {
        match self.var_id(name) {
            Some(id) => id,
            None => self.fresh_var(name),
        }
    }

    pub fn var_id(&self, name: &str) -> Option<VarId> {
        self.vars.iter().position(|v| v.name == name).map(VarId)
    }

    /// A new variable, named after `hint` (made unique).
    pub fn fresh_var(&mut self, hint: &str) -> VarId {
        let mut name = hint.to_owned();
        let mut n = 1;
        while self.var_id(&name).is_some() {
            n += 1;
            name = format!("{hint}_{n}");
        }
        self.vars.push(VarInfo { name, ..VarInfo::default() });
        VarId(self.vars.len() - 1)
    }

    pub fn bnode(&mut self) -> BNodeId {
        self.bnode_count += 1;
        BNodeId(self.bnode_count - 1)
    }

    pub fn var_name(&self, v: VarId) -> &str {
        &self.vars[v.0].name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxrdf::vocab::xsd;

    #[test]
    fn conversions() {
        let pm: PrefixMap = [("ex", "http://example.com/")].into_iter().collect();
        let iri = |t: &str| Conversion::Iri.apply(t, &pm).ok();
        let named = |i: &str| Some(NamedNode::new_unchecked(i).into());
        assert_eq!(iri("ex:a"), named("http://example.com/a"));
        assert_eq!(iri("http://x.org/y"), named("http://x.org/y"));
        // an undeclared prefix is a scheme, as in bOTTR and Lutra
        assert_eq!(iri("mailto:a@example.com"), named("mailto:a@example.com"));
        assert_eq!(iri("urn:isbn:0451450523"), named("urn:isbn:0451450523"));
        assert_eq!(iri("ex:a b"), None);
        assert_eq!(Conversion::Iri.apply("plain", &pm), Err("'plain' is not an IRI".into()));
        let dec = Conversion::Typed(xsd::DECIMAL.into());
        assert_eq!(dec.apply("9.50", &pm), Ok(Literal::new_typed_literal("9.5", xsd::DECIMAL).into()));
        assert_eq!(Conversion::Lang("en".into()).apply("hi", &pm), Ok(Literal::new_language_tagged_literal_unchecked("hi", "en").into()));
        assert!(Conversion::Lang("not a tag".into()).apply("hi", &pm).is_err());
    }

    #[test]
    fn variables_are_unique_by_name() {
        let mut p = Plan::new("t", Lifting::Given);
        let a = p.var("a");
        assert_eq!(p.var("a"), a);
        let b = p.fresh_var("a");
        assert_ne!(a, b);
        assert_eq!(p.var_name(b), "a_2");
    }
}
