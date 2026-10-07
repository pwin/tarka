//! Compiling a template to a plan.

use std::collections::HashMap;

use oxrdf::{BlankNode, NamedNode, Term};
use tarka_core::{
    BNodeId, Block, CellSource, ColumnBinding, Conversion, Expander, Lifting, Pattern, Plan, PrefixMap, Repeat, TermPat, TypeRef, Value,
    VarId,
};

use crate::model::{Instance, Kind, OTTR, OTerm, Param, Template};
use crate::{Library, MAX_DEPTH, OttrError};

/// The namespace of the annotations ottr2sparql's `decompose` writes on root templates.
pub const TQ: &str = "http://example.org/ottr-tarql#";

const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const OWL: &str = "http://www.w3.org/2002/07/owl#";

/// The OWL and RDF vocabulary types that are subtypes of `ottr:IRI`.
pub fn is_iri_type(iri: &str) -> bool {
    iri == format!("{OTTR}IRI")
        || [format!("{RDFS}Class"), format!("{RDFS}Datatype"), "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property".into()]
            .contains(&iri.to_owned())
        || ["Class", "NamedIndividual", "ObjectProperty", "DatatypeProperty", "AnnotationProperty", "Ontology"]
            .iter()
            .any(|t| iri == format!("{OWL}{t}"))
}

#[derive(Clone, Debug, Default)]
pub struct CompileOptions {
    /// The root template's values are given as instances, not read from a table.
    pub given: bool,
    /// Separators of the cells of list-typed parameters, by parameter (column) name.
    pub lists: HashMap<String, String>,
}

/// Compiles the template `name` (an IRI or a prefixed name) to a plan.
pub fn compile(lib: &Library, name: &str, options: &CompileOptions) -> Result<Plan, OttrError> {
    compile_many(lib, &[name], options)
}

/// Compiles several root templates into one plan: each row instantiates every one of
/// them. A mapping can so be put together from templates written for other mappings.
///
/// Each root's parameters read the columns of the same names. Roots share a variable
/// when they read a column the same way, and read it separately when their types
/// differ. A root's mandatory parameters drop only that root's instance.
pub fn compile_many(lib: &Library, names: &[&str], options: &CompileOptions) -> Result<Plan, OttrError> {
    let mut roots = Vec::new();
    for name in names {
        let iri = lib.resolve(name);
        let root = lib.get(&iri).ok_or_else(|| OttrError::UnknownTemplate(iri.clone()))?;
        match root.kind {
            Kind::Template => {}
            Kind::Signature => return Err(OttrError::NoPattern(iri)),
            Kind::Base => return Err(OttrError::Unsupported(format!("<{iri}> is a base template"))),
        }
        roots.push((iri, root));
    }
    if roots.is_empty() {
        return Err(OttrError::Unsupported("no root template".into()));
    }
    let mut plan = Plan::new(roots.iter().map(|(i, _)| i.as_str()).collect::<Vec<_>>().join(" "), Lifting::Given);
    plan.prefixes = lib.prefixes.clone();
    let has_tq = |t: &Template| t.annotations.iter().any(|a| a.template.starts_with(TQ));
    let mut root_vars = Vec::new();
    if roots.len() == 1 {
        let (_, root) = &roots[0];
        let vars: Vec<VarId> = root.params.iter().map(|p| plan.var(&p.name)).collect();
        plan.lifting = lifting(root, &vars, options)?;
        root_vars.push(vars);
    } else {
        if options.given {
            return Err(OttrError::Unsupported("instances name one template each".into()));
        }
        if let Some((iri, _)) = roots.iter().find(|(_, t)| has_tq(t)) {
            return Err(OttrError::Unsupported(format!("<{iri}> reads its values with tq: annotations, so it must be the only root")));
        }
        let mut bindings: Vec<ColumnBinding> = Vec::new();
        for (_, root) in &roots {
            let mut vars = Vec::new();
            for p in &root.params {
                let wanted = column_binding(VarId(0), p, options);
                let same = |b: &&ColumnBinding| {
                    b.source == wanted.source
                        && b.conversion == wanted.conversion
                        && b.list == wanted.list
                        && b.list_separator == wanted.list_separator
                };
                let var = match bindings.iter().find(same) {
                    Some(b) => b.var,
                    None => {
                        let var = plan.fresh_var(&p.name);
                        bindings.push(ColumnBinding { var, ..wanted });
                        var
                    }
                };
                vars.push(var);
            }
            root_vars.push(vars);
        }
        plan.lifting = Lifting::Columns(bindings);
    }
    let mut block = Block::default();
    for ((iri, _), vars) in roots.iter().zip(root_vars) {
        let args = vars.iter().map(|v| Sym::Pat(TermPat::Var(*v))).collect();
        Compiler { lib, plan: &mut plan }.apply(iri, args, &[], &mut block, 0)?;
    }
    plan.root = block;
    Ok(plan)
}

/// How a root template's parameters get their values.
fn lifting(root: &Template, vars: &[VarId], options: &CompileOptions) -> Result<Lifting, OttrError> {
    if options.given {
        return Ok(Lifting::Given);
    }
    let tq: Vec<&Instance> = root.annotations.iter().filter(|a| a.template.starts_with(TQ)).collect();
    if !tq.is_empty() {
        return tq_lifting(root, vars, &tq);
    }
    Ok(Lifting::Columns(root.params.iter().zip(vars).map(|(p, var)| column_binding(*var, p, options)).collect()))
}

/// Reading parameter `p` from the column of the same name. A list parameter needs a
/// separator only for text cells, which the input decides, so that is checked as cells are read.
fn column_binding(var: VarId, p: &Param, options: &CompileOptions) -> ColumnBinding {
    let source = if p.name == "ROWNUM" { CellSource::RowNumber } else { CellSource::Column(p.name.clone()) };
    let list = p.ty.as_ref().is_some_and(|t| t.is_list());
    let list_separator = if list { options.lists.get(&p.name).cloned() } else { None };
    ColumnBinding { var, source, conversion: conversion(p.ty.as_ref()), list, list_separator }
}

/// How a cell becomes a value of type `ty`.
pub fn conversion(ty: Option<&TypeRef>) -> Conversion {
    let Some(ty) = ty else { return Conversion::Plain };
    let basic = ty.basic().as_str();
    if is_iri_type(basic) {
        Conversion::Iri
    } else if basic.starts_with(XSD) && basic != format!("{XSD}string") {
        Conversion::Typed(NamedNode::new_unchecked(basic))
    } else {
        Conversion::Plain
    }
}

/// The SPARQL lifting recorded in `tq:` annotations.
fn tq_lifting(root: &Template, vars: &[VarId], tq: &[&Instance]) -> Result<Lifting, OttrError> {
    let text = |a: &Instance, i: usize| match a.args.get(i) {
        Some(OTerm::Literal(l)) => Ok(l.value().to_owned()),
        Some(OTerm::Iri(s)) => Ok(s.clone()),
        _ => Err(OttrError::Unsupported(format!("unexpected arguments in the annotation tq:{}", &a.template[TQ.len()..]))),
    };
    let (mut prefixes, mut base, mut body, mut modifiers) = (PrefixMap::new(), None, String::new(), String::new());
    for a in tq {
        match &a.template[TQ.len()..] {
            "Prefix" => prefixes.insert(text(a, 0)?, text(a, 1)?),
            "Base" => base = Some(text(a, 0)?),
            "Bind" => body.push_str(&format!("  BIND({} AS ?{})\n", text(a, 1)?, text(a, 0)?)),
            "Where" => body.push_str(&format!("  {}\n", text(a, 0)?)),
            "Modifiers" => modifiers = text(a, 0)?,
            _ => {} // tq:Source, tq:Dataset
        }
    }
    let outputs = root.params.iter().zip(vars).map(|(p, v)| (*v, p.name.clone())).collect();
    tarka_tarql::sparql_lifting(&prefixes, base.as_deref(), &body, &modifiers, outputs)
        .map(|l| Lifting::Sparql(Box::new(l)))
        .map_err(|e| OttrError::Lifting(root.iri.clone(), e))
}

/// A term during expansion.
#[derive(Clone, Debug)]
enum Sym {
    Pat(TermPat),
    None,
    /// A constant list (whose items may still be variables).
    List(Vec<Sym>),
}

struct Compiler<'a> {
    lib: &'a Library,
    plan: &'a mut Plan,
}

impl Compiler<'_> {
    fn apply(&mut self, iri: &str, args: Vec<Sym>, requires: &[VarId], block: &mut Block, depth: usize) -> Result<(), OttrError> {
        if depth > MAX_DEPTH {
            return Err(OttrError::TooDeep(iri.to_owned()));
        }
        let lib = self.lib;
        let t = lib.get(iri).ok_or_else(|| OttrError::UnknownTemplate(iri.to_owned()))?;
        if args.len() != t.params.len() {
            return Err(OttrError::Arity { template: iri.to_owned(), expected: t.params.len(), given: args.len() });
        }
        if Library::is_triple(iri) {
            if args.iter().any(|a| matches!(a, Sym::None)) {
                return Ok(());
            }
            let mut terms = args.into_iter().map(|a| pattern_term(a, iri));
            let (s, p, o) = (terms.next().unwrap()?, terms.next().unwrap()?, terms.next().unwrap()?);
            block.patterns.push(Pattern { subject: s, predicate: p, object: o, requires: requires.to_vec() });
            return Ok(());
        }
        match t.kind {
            Kind::Template => {}
            Kind::Signature => return Err(OttrError::NoPattern(iri.to_owned())),
            Kind::Base => return Err(OttrError::Unsupported(format!("base template <{iri}> (only ottr:Triple makes RDF)"))),
        }
        let mut scope = Scope { bnodes: HashMap::new(), values: HashMap::new() };
        let mut required = requires.to_vec();
        for (param, arg) in t.params.iter().zip(args) {
            let value = match arg {
                Sym::None => match &param.default {
                    Some(d) => self.term(d, &mut scope, block, iri)?,
                    None if !param.optional => return Ok(()), // a mandatory argument is none: no instance
                    None => Sym::None,
                },
                Sym::Pat(TermPat::Var(v)) => {
                    self.note(v, param);
                    match &param.default {
                        Some(d) => match self.term(d, &mut scope, block, iri)? {
                            Sym::Pat(fallback) => Sym::Pat(TermPat::Default(v, Box::new(fallback))),
                            Sym::List(items) => Sym::Pat(TermPat::Default(v, Box::new(list_pattern(items, iri)?))),
                            Sym::None => Sym::Pat(TermPat::Var(v)),
                        },
                        None => {
                            if !param.optional {
                                required.push(v);
                            }
                            Sym::Pat(TermPat::Var(v))
                        }
                    }
                }
                Sym::Pat(TermPat::Default(v, fallback)) => {
                    self.note(v, param);
                    Sym::Pat(TermPat::Default(v, fallback))
                }
                other => other,
            };
            scope.values.insert(param.name.clone(), value);
        }
        for inst in &t.body {
            let args = inst.args.iter().map(|a| self.term(a, &mut scope, block, iri)).collect::<Result<Vec<_>, _>>()?;
            match inst.expander {
                None => self.apply(&inst.template, args, &required, block, depth + 1)?,
                Some(expander) => self.expand(inst, expander, args, &required, block, depth + 1)?,
            }
        }
        Ok(())
    }

    /// An instance with a list expander: unrolled now for constant lists, a repeat for
    /// list values.
    fn expand(
        &mut self,
        inst: &Instance,
        expander: Expander,
        args: Vec<Sym>,
        requires: &[VarId],
        block: &mut Block,
        depth: usize,
    ) -> Result<(), OttrError> {
        let flagged: Vec<usize> = inst.expand.iter().enumerate().filter(|(_, f)| **f).map(|(i, _)| i).collect();
        if flagged.is_empty() {
            return Err(OttrError::Unsupported(format!("an instance of <{}> has an expander but no ++ argument", inst.template)));
        }
        if flagged.iter().any(|&i| matches!(args[i], Sym::None)) {
            return Ok(()); // nothing to expand
        }
        if flagged.iter().all(|&i| matches!(args[i], Sym::List(_))) {
            let lists: Vec<Vec<Sym>> =
                flagged.iter().map(|&i| if let Sym::List(items) = &args[i] { items.clone() } else { unreachable!() }).collect();
            for combo in combinations(expander, &lists) {
                let mut a = args.clone();
                for (&i, item) in flagged.iter().zip(combo) {
                    a[i] = item;
                }
                self.apply(&inst.template, a, requires, block, depth)?;
            }
            return Ok(());
        }
        let mut repeat = Repeat { expander, lists: Vec::new(), requires: Vec::new(), body: Block::default() };
        let mut a = args;
        for &i in &flagged {
            let Sym::Pat(TermPat::Var(list)) = a[i] else {
                return Err(OttrError::Unsupported(format!("an expander over <{}> mixes constant lists and list values", inst.template)));
            };
            let element = self.plan.fresh_var(&format!("{}_item", self.plan.var_name(list)));
            repeat.lists.push((list, element));
            a[i] = Sym::Pat(TermPat::Var(element));
        }
        self.apply(&inst.template, a, requires, &mut repeat.body, depth)?;
        block.repeats.push(repeat);
        Ok(())
    }

    /// A template-body term with the instance's parameters substituted.
    fn term(&mut self, t: &OTerm, scope: &mut Scope, block: &mut Block, template: &str) -> Result<Sym, OttrError> {
        Ok(match t {
            OTerm::Iri(i) => Sym::Pat(TermPat::Const(NamedNode::new(i.as_str()).map_err(|_| OttrError::Iri(i.clone()))?.into())),
            OTerm::Literal(l) => Sym::Pat(TermPat::Const(l.clone().into())),
            OTerm::None => Sym::None,
            OTerm::BNode(label) => {
                let id = match scope.bnodes.get(label) {
                    Some(id) => *id,
                    None => {
                        let id = self.plan.bnode();
                        block.bnodes.push(id);
                        scope.bnodes.insert(label.clone(), id);
                        id
                    }
                };
                Sym::Pat(TermPat::BNode(id))
            }
            OTerm::Var(v) => {
                scope.values.get(v).cloned().ok_or_else(|| OttrError::UnboundVariable { template: template.to_owned(), var: v.clone() })?
            }
            OTerm::List(items) => Sym::List(items.iter().map(|i| self.term(i, scope, block, template)).collect::<Result<_, _>>()?),
        })
    }

    /// Records the declared type of the parameter a variable is passed to.
    fn note(&mut self, v: VarId, param: &Param) {
        let info = &mut self.plan.vars[v.0];
        if let Some(t) = &param.ty
            && !info.types.contains(t)
        {
            info.types.push(t.clone());
        }
        info.non_blank |= param.non_blank;
    }
}

struct Scope {
    bnodes: HashMap<String, BNodeId>,
    values: HashMap<String, Sym>,
}

fn pattern_term(s: Sym, template: &str) -> Result<TermPat, OttrError> {
    match s {
        Sym::Pat(p) => Ok(p),
        Sym::List(items) => list_pattern(items, template),
        Sym::None => unreachable!("none arguments are handled before"),
    }
}

fn list_pattern(items: Vec<Sym>, template: &str) -> Result<TermPat, OttrError> {
    items
        .into_iter()
        .map(|i| match i {
            Sym::None => Err(OttrError::Unsupported(format!("none inside a list in <{template}>"))),
            other => pattern_term(other, template),
        })
        .collect::<Result<_, _>>()
        .map(TermPat::List)
}

fn combinations(expander: Expander, lists: &[Vec<Sym>]) -> Vec<Vec<Sym>> {
    match expander {
        Expander::Cross => lists.iter().fold(vec![Vec::new()], |acc, list| {
            acc.into_iter()
                .flat_map(|prefix| {
                    list.iter().map(move |item| {
                        let mut next = prefix.clone();
                        next.push(item.clone());
                        next
                    })
                })
                .collect()
        }),
        Expander::ZipMin | Expander::ZipMax => {
            let lengths = lists.iter().map(Vec::len);
            let n = if expander == Expander::ZipMin { lengths.min() } else { lengths.max() }.unwrap_or(0);
            (0..n).map(|i| lists.iter().map(|l| l.get(i).cloned().unwrap_or(Sym::None)).collect()).collect()
        }
    }
}

/// The values of a ground instance's arguments. Blank node labels are kept per
/// document (`_:x` in two instances is one node), prefixed so they cannot clash with
/// the blank nodes templates make.
pub fn instance_values(inst: &Instance) -> Result<Vec<Option<Value>>, OttrError> {
    fn value(t: &OTerm) -> Result<Option<Value>, OttrError> {
        Ok(match t {
            OTerm::Iri(i) => Some(Value::Term(NamedNode::new(i.as_str()).map_err(|_| OttrError::Iri(i.clone()))?.into())),
            OTerm::Literal(l) => Some(Value::Term(l.clone().into())),
            OTerm::BNode(b) => Some(Value::Term(Term::BlankNode(BlankNode::new_unchecked(format!("d_{}", sanitize(b)))))),
            OTerm::None => None,
            OTerm::List(items) => Some(Value::List(items.iter().map(|i| value(i)?.ok_or_else(none_in_list)).collect::<Result<_, _>>()?)),
            OTerm::Var(v) => return Err(OttrError::Unsupported(format!("an instance argument is the variable ?{v}"))),
        })
    }
    fn none_in_list() -> OttrError {
        OttrError::Unsupported("none inside a list".into())
    }
    inst.args.iter().map(value).collect()
}

fn sanitize(label: &str) -> String {
    label.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect()
}

/// A ground instance with a list expander, as the instances it stands for.
pub fn unroll_instance(inst: &Instance) -> Result<Vec<Instance>, OttrError> {
    let Some(expander) = inst.expander else { return Ok(vec![inst.clone()]) };
    let flagged: Vec<usize> = inst.expand.iter().enumerate().filter(|(_, f)| **f).map(|(i, _)| i).collect();
    let mut lists = Vec::new();
    for &i in &flagged {
        match &inst.args[i] {
            OTerm::List(items) => lists.push(items.iter().cloned().map(OSym).collect::<Vec<_>>()),
            OTerm::None => return Ok(Vec::new()),
            _ => return Err(OttrError::Unsupported(format!("a ++ argument of <{}> that is not a list", inst.template))),
        }
    }
    let n = lists.iter().map(Vec::len);
    let combos: Vec<Vec<OTerm>> = match expander {
        Expander::Cross => lists.iter().fold(vec![Vec::new()], |acc, l| {
            acc.into_iter().flat_map(|p| l.iter().map(move |i| [p.clone(), vec![i.0.clone()]].concat())).collect()
        }),
        Expander::ZipMin | Expander::ZipMax => {
            let len = if expander == Expander::ZipMin { n.min() } else { n.max() }.unwrap_or(0);
            (0..len).map(|i| lists.iter().map(|l| l.get(i).map_or(OTerm::None, |s| s.0.clone())).collect()).collect()
        }
    };
    Ok(combos
        .into_iter()
        .map(|combo| {
            let mut args = inst.args.clone();
            for (&i, item) in flagged.iter().zip(combo) {
                args[i] = item;
            }
            Instance { template: inst.template.clone(), args, expand: vec![false; inst.args.len()], expander: None, line: inst.line }
        })
        .collect())
}

#[derive(Clone)]
struct OSym(OTerm);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_stottr;
    use tarka_core::{Emitter, Labels};

    const LIB: &str = r#"
        @prefix ex: <http://ex/> . @prefix ottr: <http://ns.ottr.xyz/0.4/> .
        @prefix xsd: <http://www.w3.org/2001/XMLSchema#> . @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        ex:Labelled [ ottr:IRI ?x, ?label = "unnamed" ] :: { ottr:Triple(?x, ex:label, ?label) } .
        ex:Maker [ ottr:IRI ?p, ottr:IRI ?maker ] :: {
            ottr:Triple(?p, ex:madeBy, ?maker), ottr:Triple(_:m, ex:note, "has maker"), ottr:Triple(_:m, ex:about, ?p) } .
        ex:Skill [ ottr:IRI ?p, xsd:string ?s ] :: { ottr:Triple(?p, ex:skill, ?s) } .
        ex:Thing [ ottr:IRI ?id, ? ?label, ? ottr:IRI ?maker, ? xsd:decimal ?price, ? List<xsd:string> ?skills ] :: {
            ottr:Triple(?id, rdf:type, ex:Thing), ex:Labelled(?id, ?label), ex:Maker(?id, ?maker),
            ottr:Triple(?id, ex:price, ?price), cross | ex:Skill(?id, ++?skills),
            cross | ottr:Triple(?id, rdf:type, ++(ex:A, ex:B)), ottr:Triple(?id, ex:tags, ("x", "y")) } .
    "#;

    fn lib() -> Library {
        let mut l = Library::new();
        l.add(parse_stottr(LIB, "lib").unwrap());
        l
    }

    fn run(plan: &Plan, env: Vec<Option<Value>>) -> Vec<String> {
        let mut env = env;
        env.resize(plan.vars.len(), None);
        let mut out = Vec::new();
        Emitter::new(plan).emit(&mut env, &mut Labels::new("b"), &mut |t| out.push(t.to_string()));
        out.sort();
        out
    }

    fn iri(s: &str) -> Option<Value> {
        Some(Value::Term(NamedNode::new_unchecked(format!("http://ex/{s}")).into()))
    }

    #[test]
    fn columns_lifting_by_type() {
        let options = CompileOptions { lists: [("skills".to_owned(), ";".to_owned())].into(), ..Default::default() };
        let plan = compile(&lib(), "ex:Thing", &options).unwrap();
        let Lifting::Columns(b) = &plan.lifting else { panic!() };
        let conv: Vec<_> = b.iter().map(|b| (b.conversion.clone(), b.list_separator.clone())).collect();
        assert_eq!(conv[0], (Conversion::Iri, None));
        assert_eq!(conv[1], (Conversion::Plain, None));
        assert_eq!(conv[3], (Conversion::Typed(NamedNode::new_unchecked(format!("{XSD}decimal"))), None));
        assert_eq!(conv[4], (Conversion::Plain, Some(";".into())), "xsd:string items are plain literals");
        assert!(b[4].list && !b[0].list);
        // list cells need no separator, so none is required to compile
        let plan = compile(&lib(), "ex:Thing", &CompileOptions::default()).unwrap();
        let Lifting::Columns(b) = &plan.lifting else { panic!() };
        assert!(b[4].list && b[4].list_separator.is_none());
    }

    #[test]
    fn mandatory_defaults_lists_and_expanders() {
        let plan = compile(&lib(), "ex:Thing", &CompileOptions { given: true, ..Default::default() }).unwrap();
        let skills = Some(Value::List(vec![
            Value::Term(oxrdf::Literal::new_simple_literal("rust").into()),
            Value::Term(oxrdf::Literal::new_simple_literal("sparql").into()),
        ]));
        // no maker: the whole ex:Maker instance goes, including its blank node's triples
        let out = run(&plan, vec![iri("p1"), None, None, None, skills]);
        assert!(out.contains(&r#"<http://ex/p1> <http://ex/label> "unnamed""#.to_owned()), "{out:#?}");
        assert!(!out.iter().any(|t| t.contains("has maker")));
        assert_eq!(out.iter().filter(|t| t.contains("ex/skill>")).count(), 2);
        assert_eq!(out.iter().filter(|t| t.contains("rdf-syntax-ns#type>")).count(), 3);
        assert_eq!(out.iter().filter(|t| t.contains("rdf-syntax-ns#first>")).count(), 2);
        // a maker: its note appears; no id: nothing at all
        let out = run(&plan, vec![iri("p1"), None, iri("acme"), None, None]);
        assert!(out.iter().any(|t| t.contains("has maker")));
        assert!(run(&plan, vec![None, None, iri("acme"), None, None]).is_empty());
    }

    #[test]
    fn errors() {
        let mut l = lib();
        l.add(
            parse_stottr("@prefix ex: <http://ex/> . ex:Loop [ ?x ] :: { ex:Loop(?x) } . ex:Bad [ ?x ] :: { ex:Labelled(?y) } .", "b")
                .unwrap(),
        );
        let given = CompileOptions { given: true, ..Default::default() };
        assert!(matches!(compile(&l, "ex:Loop", &given), Err(OttrError::TooDeep(_))));
        assert!(matches!(compile(&l, "ex:Bad", &given), Err(OttrError::UnboundVariable { .. }) | Err(OttrError::Arity { .. })));
        assert!(matches!(compile(&l, "ex:Nope", &given), Err(OttrError::UnknownTemplate(_))));
    }

    #[test]
    fn instances_unroll_and_keep_document_blank_nodes() {
        let doc = parse_stottr("@prefix ex: <http://ex/> . cross | ex:Skill(_:x, ++(\"a\", \"b\")) .", "i").unwrap();
        let unrolled = unroll_instance(&doc.instances[0]).unwrap();
        assert_eq!(unrolled.len(), 2);
        let values = instance_values(&unrolled[1]).unwrap();
        assert_eq!(values[0], Some(Value::Term(BlankNode::new_unchecked("d_x").into())));
    }
}
