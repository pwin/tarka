//! SHACL shapes for the RDF a plan makes.
//!
//! Every class the plan asserts with a constant `rdf:type` gets a node shape targeting
//! it, and every constant IRI subject without a class a shape targeting that node.
//! Each predicate the plan writes on those subjects gets a property shape:
//!
//! * `sh:minCount 1` when the triple is made whenever the class triple is: it needs no
//!   variable the class triple does not, and is not inside a repeat the class triple is
//!   outside of. Where one class is asserted in several places, the triple must be made
//!   in all of them.
//! * `sh:datatype` or `sh:nodeKind`, from the OTTR types of the columns, or inferred from
//!   the expressions a TARQL query binds.
//! * `sh:class` when the object is always asserted to be of that class too, and
//!   `sh:node` when it is a blank node the plan describes without a class.
//!
//! Subjects that are variables without a constant class get no shape.

use std::collections::{BTreeMap, BTreeSet};

use oxrdf::vocab::{rdf, xsd};
use oxrdf::{BlankNode, Literal, NamedNode, Term, Triple};
use tarka_core::{BNodeId, Block, CellSource, Lifting, Pattern, Plan, PrefixMap, TermPat, VarId};

use crate::kinds::{self, Kind, Kinds};

pub const SH: &str = "http://www.w3.org/ns/shacl#";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Iri,
    BlankNode,
    Literal,
    BlankNodeOrIri,
}

impl NodeKind {
    fn local(self) -> &'static str {
        match self {
            Self::Iri => "IRI",
            Self::BlankNode => "BlankNode",
            Self::Literal => "Literal",
            Self::BlankNodeOrIri => "BlankNodeOrIRI",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Class(NamedNode),
    Node(NamedNode),
}

#[derive(Clone, Debug)]
pub struct NodeShape {
    pub iri: NamedNode,
    /// No targets: the shape is used through `sh:node`.
    pub targets: Vec<Target>,
    pub properties: Vec<PropertyShape>,
}

#[derive(Clone, Debug)]
pub struct PropertyShape {
    pub path: NamedNode,
    /// Every focus node has a value (`sh:minCount 1`).
    pub required: bool,
    pub datatype: Option<NamedNode>,
    pub node_kind: Option<NodeKind>,
    pub classes: Vec<NamedNode>,
    /// The values conform to this shape.
    pub node: Option<NamedNode>,
}

/// The shapes for one plan.
#[derive(Clone, Debug)]
pub struct Shapes {
    /// What the shapes describe (the plan's name).
    pub name: String,
    pub shapes: Vec<NodeShape>,
    /// For Turtle: `sh:`, `xsd:`, `shape:` (the base) and the plan's prefixes.
    pub prefixes: PrefixMap,
}

#[derive(Clone, Debug)]
pub struct ShapeOptions {
    /// The namespace of the shapes' IRIs.
    pub base: String,
}

impl Default for ShapeOptions {
    fn default() -> Self {
        Self { base: "urn:tarka:shapes:".into() }
    }
}

/// The shapes for what `plan` makes.
pub fn shapes(plan: &Plan, options: &ShapeOptions) -> Shapes {
    let mut prefixes = PrefixMap::new();
    prefixes.insert("sh", SH);
    prefixes.insert("xsd", xsd::STRING.as_str().trim_end_matches("string"));
    prefixes.insert("shape", options.base.as_str());
    for (p, ns) in plan.prefixes.iter() {
        prefixes.insert_if_absent(p, ns);
    }
    let mut g = Generator::new(plan, &options.base);
    let mut shapes = Vec::new();
    for (class, assertions) in g.classes.clone() {
        let local = g.local(&class);
        let iri = g.names.mint(&local);
        let contexts: Vec<Context> = assertions.iter().map(|&o| g.context_of(o)).collect();
        let properties = g.properties(&iri, &contexts);
        shapes.push(NodeShape { iri, targets: vec![Target::Class(class)], properties });
    }
    for node in g.untyped_iris() {
        let local = g.local(&node);
        let iri = g.names.mint(&local);
        let contexts = [Context { subject: Subject::Iri(node.clone()), block: 0, needs: BTreeSet::new() }];
        let properties = g.properties(&iri, &contexts);
        shapes.push(NodeShape { iri, targets: vec![Target::Node(node)], properties });
    }
    shapes.append(&mut g.node_shapes);
    Shapes { name: plan.name.clone(), shapes, prefixes }
}

/// What a triple pattern's subject is, when it can have a shape.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Subject {
    Var(VarId),
    BNode(BNodeId),
    Iri(NamedNode),
}

fn subject_of(t: &TermPat) -> Option<Subject> {
    match t {
        TermPat::Var(v) => Some(Subject::Var(*v)),
        TermPat::BNode(b) => Some(Subject::BNode(*b)),
        TermPat::Const(Term::NamedNode(n)) => Some(Subject::Iri(n.clone())),
        _ => None,
    }
}

/// A triple pattern, the block it is in and the variables it needs bound.
struct Occurrence<'a> {
    pattern: &'a Pattern,
    block: usize,
    needs: BTreeSet<VarId>,
}

/// Where a subject is known to exist: its triples made whenever these needs are
/// bound in this block are always there.
#[derive(Clone, Debug)]
struct Context {
    subject: Subject,
    block: usize,
    needs: BTreeSet<VarId>,
}

struct Generator<'a> {
    plan: &'a Plan,
    vars: Vec<Kinds>,
    occurrences: Vec<Occurrence<'a>>,
    /// Each block's enclosing block (the root block is 0).
    parents: Vec<Option<usize>>,
    /// Variables bound in every solution (the row number).
    always: BTreeSet<VarId>,
    /// Each class, and the occurrences asserting it.
    classes: BTreeMap<NamedNode, Vec<usize>>,
    names: Names,
    /// Shapes for blank nodes described without a class.
    node_shapes: Vec<NodeShape>,
    /// The blank nodes whose shapes are being made (a blank node can describe itself).
    making: Vec<BNodeId>,
}

impl<'a> Generator<'a> {
    fn new(plan: &'a Plan, base: &str) -> Self {
        let mut g = Self {
            plan,
            vars: kinds::var_kinds(plan),
            occurrences: Vec::new(),
            parents: vec![None],
            always: always_bound(plan),
            classes: BTreeMap::new(),
            names: Names { base: base.to_owned(), used: BTreeSet::new() },
            node_shapes: Vec::new(),
            making: Vec::new(),
        };
        g.walk(&plan.root, 0, &BTreeSet::new());
        for (i, o) in g.occurrences.iter().enumerate() {
            if subject_of(&o.pattern.subject).is_some()
                && o.pattern.predicate == TermPat::Const(rdf::TYPE.into_owned().into())
                && let TermPat::Const(Term::NamedNode(class)) = &o.pattern.object
            {
                g.classes.entry(class.clone()).or_default().push(i);
            }
        }
        g
    }

    fn walk(&mut self, block: &'a Block, id: usize, inherited: &BTreeSet<VarId>) {
        for p in &block.patterns {
            let mut needs = inherited.clone();
            for t in [&p.subject, &p.predicate, &p.object] {
                term_needs(t, &mut needs);
            }
            needs.extend(p.requires.iter().copied());
            self.occurrences.push(Occurrence { pattern: p, block: id, needs });
        }
        for r in &block.repeats {
            let child = self.parents.len();
            self.parents.push(Some(id));
            let mut needs = inherited.clone();
            needs.extend(r.requires.iter().copied());
            needs.extend(r.lists.iter().flat_map(|(list, element)| [*list, *element]));
            self.walk(&r.body, child, &needs);
        }
    }

    /// Whether block `outer` is `inner` or encloses it.
    fn encloses(&self, outer: usize, mut inner: usize) -> bool {
        loop {
            if inner == outer {
                return true;
            }
            match self.parents[inner] {
                Some(p) => inner = p,
                None => return false,
            }
        }
    }

    /// Whether occurrence `o` is made whenever context `c` holds.
    fn always_with(&self, o: &Occurrence, c: &Context) -> bool {
        self.encloses(o.block, c.block) && o.needs.iter().all(|v| c.needs.contains(v) || self.always.contains(v))
    }

    fn context_of(&self, o: usize) -> Context {
        let o = &self.occurrences[o];
        Context { subject: subject_of(&o.pattern.subject).expect("a class assertion's subject"), block: o.block, needs: o.needs.clone() }
    }

    /// Constant IRI subjects that are not asserted to be of a class.
    fn untyped_iris(&self) -> Vec<NamedNode> {
        let typed: BTreeSet<Subject> =
            self.classes.values().flatten().filter_map(|&o| subject_of(&self.occurrences[o].pattern.subject)).collect();
        let iris: BTreeSet<NamedNode> = self
            .occurrences
            .iter()
            .filter_map(|o| match subject_of(&o.pattern.subject) {
                Some(s @ Subject::Iri(_)) if !typed.contains(&s) => Some(s),
                _ => None,
            })
            .filter_map(|s| match s {
                Subject::Iri(n) => Some(n),
                _ => None,
            })
            .collect();
        iris.into_iter().collect()
    }

    /// The classes the value of `object` is always asserted to have when `at` is made.
    fn classes_of(&self, object: &TermPat, at: &Occurrence) -> BTreeSet<NamedNode> {
        let Some(subject) = subject_of(object) else { return BTreeSet::new() };
        let here = Context { subject: subject.clone(), block: at.block, needs: at.needs.clone() };
        self.classes
            .iter()
            .filter(|(_, assertions)| {
                assertions.iter().any(|&a| {
                    let a = &self.occurrences[a];
                    subject_of(&a.pattern.subject).as_ref() == Some(&subject) && self.always_with(a, &here)
                })
            })
            .map(|(class, _)| class.clone())
            .collect()
    }

    /// The property shapes of a subject known in each of `contexts`.
    fn properties(&mut self, shape: &NamedNode, contexts: &[Context]) -> Vec<PropertyShape> {
        // predicate → (context, occurrence, made whenever the context holds)
        let mut by_predicate: BTreeMap<NamedNode, Vec<(usize, usize, bool)>> = BTreeMap::new();
        for (c, context) in contexts.iter().enumerate() {
            for (i, o) in self.occurrences.iter().enumerate() {
                let TermPat::Const(Term::NamedNode(p)) = &o.pattern.predicate else { continue };
                if subject_of(&o.pattern.subject).as_ref() != Some(&context.subject) || p.as_ref() == rdf::TYPE {
                    continue;
                }
                by_predicate.entry(p.clone()).or_default().push((c, i, self.always_with(o, context)));
            }
        }
        let mut out = Vec::new();
        for (path, found) in by_predicate {
            let required = (0..contexts.len()).all(|c| found.iter().any(|&(fc, _, always)| fc == c && always));
            let mut kinds: Kinds = Some(BTreeSet::new());
            let mut classes: Option<BTreeSet<NamedNode>> = None;
            for &(_, i, _) in &found {
                let o = &self.occurrences[i];
                kinds = kinds::join(kinds, kinds::term_kinds(&o.pattern.object, &self.vars));
                let here = self.classes_of(&o.pattern.object, o);
                classes = Some(match classes {
                    None => here,
                    Some(c) => c.intersection(&here).cloned().collect(),
                });
            }
            let classes: Vec<NamedNode> = classes.unwrap_or_default().into_iter().collect();
            let (datatype, node_kind) = constraint(&kinds);
            // a blank node described here, without a class, gets a shape of its own
            let node = match found.as_slice() {
                [(_, i, _)] if classes.is_empty() => {
                    let o = &self.occurrences[*i];
                    match kinds::bnode(&o.pattern.object).filter(|b| !self.making.contains(b)) {
                        Some(b) => {
                            let context = Context { subject: Subject::BNode(b), block: o.block, needs: o.needs.clone() };
                            let local = local_name(path.as_str());
                            let iri = self.names.mint(&format!("{}-{local}", self.names.local_of(shape)));
                            self.making.push(b);
                            let properties = self.properties(&iri, &[context]);
                            self.making.pop();
                            if properties.is_empty() {
                                None
                            } else {
                                self.node_shapes.push(NodeShape { iri: iri.clone(), targets: Vec::new(), properties });
                                Some(iri)
                            }
                        }
                        None => None,
                    }
                }
                _ => None,
            };
            out.push(PropertyShape { path, required, datatype, node_kind, classes, node });
        }
        out
    }

    /// A readable local name for a class or node, unique among the shapes.
    fn local(&self, iri: &NamedNode) -> String {
        let simple = local_name(iri.as_str());
        let clash = self.classes.keys().any(|c| c != iri && local_name(c.as_str()) == simple);
        match self.plan.prefixes.compact(iri.as_str()) {
            Some(pname) if clash => pname.replace(':', "-"),
            _ => simple,
        }
    }
}

/// Variables bound in every solution: the row number.
fn always_bound(plan: &Plan) -> BTreeSet<VarId> {
    match &plan.lifting {
        Lifting::Columns(bindings) => bindings.iter().filter(|b| b.source == CellSource::RowNumber).map(|b| b.var).collect(),
        Lifting::Sparql(lifting) => lifting.outputs.iter().filter(|(_, v)| v.as_str() == "ROWNUM").map(|(id, _)| *id).collect(),
        Lifting::Given => BTreeSet::new(),
    }
}

/// The variables a term pattern needs bound to give a value.
fn term_needs(t: &TermPat, out: &mut BTreeSet<VarId>) {
    match t {
        TermPat::Var(v) => {
            out.insert(*v);
        }
        // an unbound variable takes the default
        TermPat::Default(_, fallback) => term_needs(fallback, out),
        TermPat::List(items) => items.iter().for_each(|i| term_needs(i, out)),
        TermPat::Const(_) | TermPat::BNode(_) => {}
    }
}

/// `sh:datatype` or `sh:nodeKind` for values of these kinds.
fn constraint(kinds: &Kinds) -> (Option<NamedNode>, Option<NodeKind>) {
    let Some(kinds) = kinds.as_ref().filter(|k| !k.is_empty()) else { return (None, None) };
    if kinds.iter().all(|k| matches!(k, Kind::Literal(_) | Kind::AnyLiteral)) {
        return match kinds.iter().next() {
            Some(Kind::Literal(dt)) if kinds.len() == 1 => (Some(dt.clone()), None),
            _ => (None, Some(NodeKind::Literal)),
        };
    }
    let node_kind = if kinds.iter().all(|k| *k == Kind::Iri) {
        Some(NodeKind::Iri)
    } else if kinds.iter().all(|k| *k == Kind::BlankNode) {
        Some(NodeKind::BlankNode)
    } else if kinds.iter().all(|k| matches!(k, Kind::Iri | Kind::BlankNode | Kind::List)) {
        Some(NodeKind::BlankNodeOrIri)
    } else {
        None
    };
    (None, node_kind)
}

/// The last segment of an IRI, made safe for a Turtle local name.
fn local_name(iri: &str) -> String {
    let last = iri.trim_end_matches(['#', '/']).rsplit(['#', '/', ':']).next().unwrap_or_default();
    let mut s: String = last.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    if !s.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
        s.insert(0, '_');
    }
    s
}

/// The shapes' IRIs: the base, a local name and `Shape`.
struct Names {
    base: String,
    used: BTreeSet<String>,
}

impl Names {
    fn mint(&mut self, local: &str) -> NamedNode {
        let mut name = format!("{local}Shape");
        let mut n = 1;
        while self.used.contains(&name) {
            n += 1;
            name = format!("{local}Shape{n}");
        }
        self.used.insert(name.clone());
        NamedNode::new_unchecked(format!("{}{name}", self.base))
    }

    /// The local name a shape was minted with (without `Shape`).
    fn local_of(&self, shape: &NamedNode) -> String {
        let name = shape.as_str().strip_prefix(&self.base).unwrap_or(shape.as_str());
        name.rsplit_once("Shape").map_or(name, |(l, _)| l).to_owned()
    }
}

impl Shapes {
    /// The shapes as triples.
    pub fn triples(&self) -> Vec<Triple> {
        let sh = |local: &str| NamedNode::new_unchecked(format!("{SH}{local}"));
        let mut out = Vec::new();
        let mut n = 0;
        for shape in &self.shapes {
            out.push(Triple::new(shape.iri.clone(), rdf::TYPE, sh("NodeShape")));
            for t in &shape.targets {
                match t {
                    Target::Class(c) => out.push(Triple::new(shape.iri.clone(), sh("targetClass"), c.clone())),
                    Target::Node(node) => out.push(Triple::new(shape.iri.clone(), sh("targetNode"), node.clone())),
                }
            }
            for p in &shape.properties {
                let node = BlankNode::new_unchecked(format!("p{n}"));
                n += 1;
                out.push(Triple::new(shape.iri.clone(), sh("property"), node.clone()));
                out.push(Triple::new(node.clone(), sh("path"), p.path.clone()));
                if p.required {
                    out.push(Triple::new(node.clone(), sh("minCount"), Literal::new_typed_literal("1", xsd::INTEGER)));
                }
                if let Some(dt) = &p.datatype {
                    out.push(Triple::new(node.clone(), sh("datatype"), dt.clone()));
                }
                if let Some(k) = p.node_kind {
                    out.push(Triple::new(node.clone(), sh("nodeKind"), sh(k.local())));
                }
                for c in &p.classes {
                    out.push(Triple::new(node.clone(), sh("class"), c.clone()));
                }
                if let Some(s) = &p.node {
                    out.push(Triple::new(node.clone(), sh("node"), s.clone()));
                }
            }
        }
        out
    }

    /// The shapes as Turtle, with each shape's property shapes nested in it.
    pub fn to_turtle(&self) -> String {
        let iri = |n: &NamedNode| self.prefixes.compact(n.as_str()).unwrap_or_else(|| format!("<{}>", n.as_str()));
        let mut out = String::new();
        for (p, ns) in self.prefixes.iter() {
            out.push_str(&format!("@prefix {p}: <{ns}> .\n"));
        }
        out.push_str(&format!("\n# Shapes for what {} makes, generated by tarka.\n", self.name));
        for shape in &self.shapes {
            out.push_str(&format!("\n{} a sh:NodeShape", iri(&shape.iri)));
            for t in &shape.targets {
                match t {
                    Target::Class(c) => out.push_str(&format!(" ;\n    sh:targetClass {}", iri(c))),
                    Target::Node(node) => out.push_str(&format!(" ;\n    sh:targetNode {}", iri(node))),
                }
            }
            for (i, p) in shape.properties.iter().enumerate() {
                out.push_str(if i == 0 { " ;\n    sh:property\n        [ " } else { " ,\n        [ " });
                let mut parts = vec![format!("sh:path {}", iri(&p.path))];
                if p.required {
                    parts.push("sh:minCount 1".into());
                }
                if let Some(dt) = &p.datatype {
                    parts.push(format!("sh:datatype {}", iri(dt)));
                }
                if let Some(k) = p.node_kind {
                    parts.push(format!("sh:nodeKind sh:{}", k.local()));
                }
                for c in &p.classes {
                    parts.push(format!("sh:class {}", iri(c)));
                }
                if let Some(s) = &p.node {
                    parts.push(format!("sh:node {}", iri(s)));
                }
                out.push_str(&parts.join(" ; "));
                out.push_str(" ]");
            }
            out.push_str(" .\n");
        }
        out
    }
}
