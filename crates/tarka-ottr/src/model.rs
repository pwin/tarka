//! The OTTR template model.

use oxrdf::Literal;
use tarka_core::{Expander, PrefixMap, TypeRef};

pub const OTTR: &str = "http://ns.ottr.xyz/0.4/";
pub const OTTR_TRIPLE: &str = "http://ns.ottr.xyz/0.4/Triple";
pub const OTTR_IRI: &str = "http://ns.ottr.xyz/0.4/IRI";

/// A term in a template body, an argument or a default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OTerm {
    Iri(String),
    BNode(String),
    Literal(Literal),
    Var(String),
    None,
    List(Vec<OTerm>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub ty: Option<TypeRef>,
    pub optional: bool,
    pub non_blank: bool,
    pub default: Option<OTerm>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instance {
    pub template: String,
    pub args: Vec<OTerm>,
    /// Which arguments are marked `++` (expanded).
    pub expand: Vec<bool>,
    pub expander: Option<Expander>,
    pub line: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A template with a pattern.
    Template,
    /// A base template (`:: BASE`).
    Base,
    /// A signature only.
    Signature,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template {
    pub iri: String,
    pub params: Vec<Param>,
    pub body: Vec<Instance>,
    pub annotations: Vec<Instance>,
    pub kind: Kind,
    pub line: usize,
}

impl Template {
    pub fn param(&self, name: &str) -> Option<&Param> {
        self.params.iter().find(|p| p.name == name)
    }

    /// `ottr:Triple`, the one base template tarka expands to RDF.
    pub fn triple() -> Self {
        let iri = |s: &str| Some(TypeRef::Basic(oxrdf::NamedNode::new_unchecked(s)));
        let param = |name: &str, ty, non_blank| Param { name: name.into(), ty, optional: false, non_blank, default: None };
        Self {
            iri: OTTR_TRIPLE.into(),
            params: vec![
                param("subject", iri(OTTR_IRI), false),
                param("predicate", iri(OTTR_IRI), true),
                param("object", iri("http://www.w3.org/2000/01/rdf-schema#Resource"), false),
            ],
            body: Vec::new(),
            annotations: Vec::new(),
            kind: Kind::Base,
            line: 0,
        }
    }
}

/// A parsed stOTTR document: templates and instances.
#[derive(Clone, Debug, Default)]
pub struct Document {
    pub prefixes: PrefixMap,
    pub templates: Vec<Template>,
    pub instances: Vec<Instance>,
}
