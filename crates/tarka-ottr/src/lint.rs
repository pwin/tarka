//! A linter for template libraries.
//!
//! It reports what Lutra's linter reports, with the same severities:
//!
//! * errors: an undefined template, the wrong number of arguments, a cycle, an argument
//!   whose type does not fit its parameter, and one argument given to parameters of
//!   incompatible types;
//! * warnings: an unused parameter, and a template defined more than once.
//!
//! and some it does not: a variable that is not a parameter, a blank node for a
//! non-blank parameter, an expander without `++` (or `++` without an expander), a
//! literal subject or a predicate that is not an IRI in `ottr:Triple` (errors), a type
//! OTTR does not have and a default that does not fit its parameter (warnings).
//!
//! The arguments of `ottr:Triple` are not type-checked otherwise, as in Lutra: its
//! parameters accept anything a pattern can give them.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use tarka_core::{PrefixMap, TypeRef};

use crate::library::Library;
use crate::model::{Instance, Kind, OTerm, Template};
use crate::types::{self, BOT, IRI, RESOURCE, basic, compatible, consistent, display, effective};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error,
    Warning,
}

/// What a finding is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Check {
    UndefinedTemplate,
    Arity,
    Cycle,
    Type,
    InconsistentUses,
    UnusedParameter,
    Duplicate,
    UndeclaredVariable,
    NonBlank,
    Expander,
    Triple,
    UnknownType,
    Default,
}

impl Check {
    pub fn name(self) -> &'static str {
        match self {
            Self::UndefinedTemplate => "undefined-template",
            Self::Arity => "arity",
            Self::Cycle => "cycle",
            Self::Type => "type",
            Self::InconsistentUses => "inconsistent-uses",
            Self::UnusedParameter => "unused-parameter",
            Self::Duplicate => "duplicate",
            Self::UndeclaredVariable => "undeclared-variable",
            Self::NonBlank => "non-blank",
            Self::Expander => "expander",
            Self::Triple => "triple",
            Self::UnknownType => "unknown-type",
            Self::Default => "default",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    pub file: String,
    pub line: usize,
    pub severity: Severity,
    pub check: Check,
    /// The template the finding is in.
    pub template: String,
    pub message: String,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{}:{}: {severity}: {} [{}]", self.file, self.line, self.message, self.check.name())
    }
}

/// Everything wrong with the templates of `lib`, sorted by file and line.
pub fn lint(lib: &Library) -> Vec<Finding> {
    let mut out = Linter { lib, prefixes: &lib.prefixes, findings: Vec::new() };
    for t in lib.templates().filter(|t| !Library::is_triple(&t.iri)) {
        out.template(t);
    }
    out.cycles();
    for (iri, places) in lib.duplicates() {
        for (file, line) in places {
            let others: Vec<String> = places.iter().filter(|p| (&p.0, p.1) != (file, *line)).map(|(f, l)| format!("{f}:{l}")).collect();
            let message = format!("{} is defined more than once (also at {})", out.name(iri), others.join(", "));
            out.findings.push(Finding {
                file: file.clone(),
                line: *line,
                severity: Severity::Warning,
                check: Check::Duplicate,
                template: iri.to_owned(),
                message,
            });
        }
    }
    let mut findings = out.findings;
    findings.sort();
    findings.dedup();
    findings
}

/// Where one argument is used: what it is given to.
struct Use<'a> {
    param: TypeRef,
    instance: &'a Instance,
    index: usize,
}

struct Linter<'a> {
    lib: &'a Library,
    prefixes: &'a PrefixMap,
    findings: Vec<Finding>,
}

impl Linter<'_> {
    fn name(&self, iri: &str) -> String {
        self.prefixes.compact(iri).unwrap_or_else(|| format!("<{iri}>"))
    }

    fn ty(&self, t: &TypeRef) -> String {
        display(t, self.prefixes)
    }

    fn push(&mut self, t: &Template, line: usize, severity: Severity, check: Check, message: String) {
        self.findings.push(Finding { file: t.file.clone(), line, severity, check, template: t.iri.clone(), message });
    }

    fn term(&self, term: &OTerm) -> String {
        match term {
            OTerm::Iri(i) => self.name(i),
            OTerm::BNode(b) => format!("_:{b}"),
            OTerm::Literal(l) => match self.prefixes.compact(l.datatype().as_str()) {
                Some(dt) if l.language().is_none() && l.datatype() != oxrdf::vocab::xsd::STRING => {
                    format!("\"{}\"^^{dt}", l.value())
                }
                _ => l.to_string(),
            },
            OTerm::Var(v) => format!("?{v}"),
            OTerm::None => "none".into(),
            OTerm::List(items) => format!("({})", items.iter().map(|i| self.term(i)).collect::<Vec<_>>().join(", ")),
        }
    }

    fn template(&mut self, t: &Template) {
        for p in &t.params {
            if let Some(unknown) = p.ty.as_ref().and_then(unknown_type) {
                let message = format!("{} (the type of ?{}) is not an OTTR type: it is read as rdfs:Resource", self.name(&unknown), p.name);
                self.push(t, t.line, Severity::Warning, Check::UnknownType, message);
            }
        }
        if t.kind != Kind::Template {
            return;
        }
        let types: HashMap<&str, TypeRef> = t.params.iter().map(|p| (p.name.as_str(), effective(p.ty.as_ref()))).collect();
        for p in &t.params {
            if let Some(default) = &p.default
                && let Some(dt) = term_type(default, &types)
                && !compatible(&dt, &types[p.name.as_str()])
            {
                let message =
                    format!("the default {} of ?{} does not fit its type {}", self.term(default), p.name, self.ty(&types[p.name.as_str()]));
                self.push(t, t.line, Severity::Warning, Check::Default, message);
            }
        }
        let mut used = BTreeSet::new();
        for inst in &t.body {
            inst.args.iter().for_each(|a| vars(a, &mut used));
        }
        for p in t.params.iter().filter(|p| !used.contains(p.name.as_str())) {
            self.push(t, t.line, Severity::Warning, Check::UnusedParameter, format!("parameter ?{} is not used in the pattern", p.name));
        }
        let mut uses: BTreeMap<String, Vec<Use>> = BTreeMap::new();
        for inst in &t.body {
            self.instance(t, inst, &types, &mut uses);
        }
        for (arg, uses) in &uses {
            for (i, a) in uses.iter().enumerate() {
                for b in &uses[i + 1..] {
                    if !consistent(&a.param, &b.param) {
                        let message = format!(
                            "{arg} is given to parameters of incompatible types: {} ({}, argument {}) and {} ({}, argument {})",
                            self.ty(&a.param),
                            self.name(&a.instance.template),
                            a.index + 1,
                            self.ty(&b.param),
                            self.name(&b.instance.template),
                            b.index + 1,
                        );
                        self.push(t, b.instance.line, Severity::Error, Check::InconsistentUses, message);
                    }
                }
            }
        }
    }

    fn instance<'i>(
        &mut self,
        t: &Template,
        inst: &'i Instance,
        types: &HashMap<&str, TypeRef>,
        uses: &mut BTreeMap<String, Vec<Use<'i>>>,
    ) {
        let line = inst.line;
        for v in inst.args.iter().flat_map(|a| {
            let mut s = BTreeSet::new();
            vars(a, &mut s);
            s
        }) {
            if !types.contains_key(v) {
                self.push(t, line, Severity::Error, Check::UndeclaredVariable, format!("?{v} is not a parameter of {}", self.name(&t.iri)));
            }
        }
        let Some(callee) = self.lib.get(&inst.template) else {
            let message = format!("{} is not defined", self.name(&inst.template));
            self.push(t, line, Severity::Error, Check::UndefinedTemplate, message);
            return;
        };
        if inst.args.len() != callee.params.len() {
            let message = format!(
                "{} takes {} argument{}, but is given {}",
                self.name(&callee.iri),
                callee.params.len(),
                if callee.params.len() == 1 { "" } else { "s" },
                inst.args.len()
            );
            self.push(t, line, Severity::Error, Check::Arity, message);
            return;
        }
        let expanded = inst.expand.iter().any(|e| *e);
        match (inst.expander.is_some(), expanded) {
            (true, false) => self.push(t, line, Severity::Error, Check::Expander, "an expander needs a ++ argument".into()),
            (false, true) => {
                self.push(t, line, Severity::Error, Check::Expander, "a ++ argument needs an expander (cross, zipMin or zipMax)".into())
            }
            _ => {}
        }
        let triple = Library::is_triple(&callee.iri);
        for (i, (arg, param)) in inst.args.iter().zip(&callee.params).enumerate() {
            let flagged = inst.expand.get(i).copied().unwrap_or(false);
            if param.non_blank && matches!(arg, OTerm::BNode(_)) {
                let message = format!("a blank node is given to the non-blank parameter ?{} of {}", param.name, self.name(&callee.iri));
                self.push(t, line, Severity::Error, Check::NonBlank, message);
            }
            if triple {
                self.triple_argument(t, line, i, arg, types);
                continue;
            }
            let mut ptype = effective(param.ty.as_ref());
            if flagged {
                ptype = TypeRef::List(Box::new(ptype));
                if let Some(at) = term_type(arg, types).filter(|a| !matches!(a, TypeRef::List(_) | TypeRef::NeList(_)))
                    && !matches!(arg, OTerm::None)
                {
                    let message = format!("{} is expanded with ++ but is not a list ({})", self.term(arg), self.ty(&at));
                    self.push(t, line, Severity::Error, Check::Expander, message);
                    continue;
                }
            }
            self.argument(t, inst, i, arg, &ptype, types);
            if !matches!(arg, OTerm::None | OTerm::List(_)) {
                uses.entry(self.term(arg)).or_default().push(Use { param: ptype, instance: inst, index: i });
            }
        }
    }

    /// Checks one argument (a list constant item by item) against a parameter type.
    fn argument(&mut self, t: &Template, inst: &Instance, i: usize, arg: &OTerm, ptype: &TypeRef, types: &HashMap<&str, TypeRef>) {
        if let (OTerm::List(items), TypeRef::List(element) | TypeRef::NeList(element)) = (arg, ptype) {
            for item in items {
                self.argument(t, inst, i, item, element, types);
            }
            return;
        }
        let Some(at) = term_type(arg, types) else { return };
        if !compatible(&at, ptype) {
            let message = format!(
                "{} ({}) does not fit the type {} of argument {} of {}",
                self.term(arg),
                self.ty(&at),
                self.ty(ptype),
                i + 1,
                self.name(&inst.template)
            );
            self.push(t, inst.line, Severity::Error, Check::Type, message);
        }
    }

    /// `ottr:Triple(subject, predicate, object)`: what can never make a triple.
    fn triple_argument(&mut self, t: &Template, line: usize, i: usize, arg: &OTerm, types: &HashMap<&str, TypeRef>) {
        let literal = |a: &OTerm| match a {
            OTerm::Literal(_) => true,
            OTerm::Var(_) => term_type(a, types).is_some_and(|at| at != basic(BOT) && types::subtype(&at, &basic(types::LITERAL))),
            _ => false,
        };
        let message = match i {
            0 if literal(arg) || matches!(arg, OTerm::List(_)) => Some(format!("{} cannot be the subject of a triple", self.term(arg))),
            1 if literal(arg) || matches!(arg, OTerm::BNode(_) | OTerm::List(_)) => {
                Some(format!("{} cannot be the predicate of a triple: it must be an IRI", self.term(arg)))
            }
            _ => None,
        };
        if let Some(message) = message {
            self.push(t, line, Severity::Error, Check::Triple, message);
        }
    }

    /// Every template in a cycle of instances.
    fn cycles(&mut self) {
        let lib = self.lib;
        let graph: BTreeMap<&str, BTreeSet<&str>> = lib
            .templates()
            .filter(|t| t.kind == Kind::Template)
            .map(|t| (t.iri.as_str(), t.body.iter().map(|i| i.template.as_str()).filter(|c| lib.get(c).is_some()).collect()))
            .collect();
        for (iri, callees) in &graph {
            // reachable from its callees back to itself?
            let mut seen = BTreeSet::new();
            let mut stack: Vec<&str> = callees.iter().copied().collect();
            let mut cyclic = false;
            while let Some(n) = stack.pop() {
                if n == *iri {
                    cyclic = true;
                    break;
                }
                if seen.insert(n)
                    && let Some(next) = graph.get(n)
                {
                    stack.extend(next.iter().copied());
                }
            }
            if cyclic {
                let t = lib.get(iri).expect("a template in the library");
                let message = format!("{} depends on itself through its instances", self.name(iri));
                self.push(t, t.line, Severity::Error, Check::Cycle, message);
            }
        }
    }
}

/// The variables in a term (lists included).
fn vars<'a>(t: &'a OTerm, out: &mut BTreeSet<&'a str>) {
    match t {
        OTerm::Var(v) => {
            out.insert(v);
        }
        OTerm::List(items) => items.iter().for_each(|i| vars(i, out)),
        _ => {}
    }
}

/// A basic type in `t` that OTTR does not have.
fn unknown_type(t: &TypeRef) -> Option<String> {
    match t {
        TypeRef::Basic(n) => (!types::is_known(n.as_str())).then(|| n.as_str().to_owned()),
        TypeRef::List(i) | TypeRef::NeList(i) | TypeRef::Lub(i) => unknown_type(i),
    }
}

/// The type of a term in a template body (None for a variable that is not a parameter).
pub fn term_type(t: &OTerm, params: &HashMap<&str, TypeRef>) -> Option<TypeRef> {
    Some(match t {
        OTerm::Iri(_) => TypeRef::Lub(Box::new(basic(IRI))),
        OTerm::BNode(_) => TypeRef::Lub(Box::new(basic(RESOURCE))),
        OTerm::Literal(l) => literal_type(l),
        OTerm::None => basic(BOT),
        OTerm::Var(v) => params.get(v.as_str())?.clone(),
        OTerm::List(items) => {
            let mut element: Option<TypeRef> = None;
            for i in items {
                let it = term_type(i, params)?;
                element = Some(match element {
                    None => it,
                    Some(e) => types::join(&e, &it),
                });
            }
            match element {
                Some(e) => TypeRef::NeList(Box::new(e)),
                None => TypeRef::List(Box::new(basic(BOT))),
            }
        }
    })
}

/// The type of a literal: its datatype.
pub fn literal_type(l: &oxrdf::Literal) -> TypeRef {
    if l.language().is_some() { basic("http://www.w3.org/1999/02/22-rdf-syntax-ns#langString") } else { basic(l.datatype().as_str()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_stottr;

    fn lint_text(text: &str) -> Vec<Finding> {
        let mut lib = Library::new();
        lib.add(parse_stottr(text, "lib.stottr").unwrap());
        lint(&lib)
    }

    const PREFIXES: &str = "@prefix ex: <http://example.com/ns#> . @prefix ottr: <http://ns.ottr.xyz/0.4/> .
        @prefix xsd: <http://www.w3.org/2001/XMLSchema#> . @prefix owl: <http://www.w3.org/2002/07/owl#> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> . @prefix schema: <https://schema.org/> .
        ex:Named [ ottr:IRI ?x ] :: { ottr:Triple(?x, rdfs:label, \"x\") } .\n";

    fn checks(text: &str) -> Vec<(Check, String)> {
        let findings = lint_text(&format!("{PREFIXES}{text}"));
        findings.into_iter().map(|f| (f.check, f.template.rsplit('#').next().unwrap().to_owned())).collect()
    }

    #[test]
    fn a_clean_library_has_no_findings() {
        let text = "ex:Fine [ owl:Class ?c, xsd:integer ?i, List<xsd:string> ?l, ? ?any ] :: {
              ex:Named(?c), ex:Dec(?i), ex:Strings(?l), ex:Dec(1), ex:Strings((\"a\", \"b\")), ex:Named(none),
              cross | ex:Str(++?l), ottr:Triple(?c, rdfs:comment, ?any) } .
            ex:Dec [ xsd:decimal ?d ] :: { ottr:Triple(ex:s, ex:p, ?d) } .
            ex:Str [ xsd:string ?s ] :: { ottr:Triple(ex:s, ex:p, ?s) } .
            ex:Strings [ List<xsd:string> ?l ] :: { ottr:Triple(ex:s, ex:p, ?l) } .";
        assert_eq!(checks(text), []);
    }

    #[test]
    fn what_lutra_reports() {
        let text = "ex:BadType [ xsd:integer ?n ] :: { ex:Named(?n) } .
            ex:Unused [ ottr:IRI ?a, ?b ] :: { ottr:Triple(?a, rdfs:label, \"a\"), ex:Named(\"not an IRI\") } .
            ex:Missing [ ottr:IRI ?a ] :: { ex:Nowhere(?a) } .
            ex:Arity [ ottr:IRI ?a ] :: { ex:Named(?a, ?a) } .
            ex:Ping [ ottr:IRI ?a ] :: { ex:Pong(?a) } .
            ex:Pong [ ottr:IRI ?a ] :: { ex:Ping(?a) } .
            ex:E1 [ ?u ] :: { ex:Named(?u) } .
            ex:E4 [ ] :: { ex:Named(_:b), ex:Str(_:b) } .
            ex:Str [ xsd:string ?s ] :: { ottr:Triple(ex:s, ex:p, ?s) } .";
        let mut found = checks(text);
        found.sort();
        let mut expected: Vec<(Check, String)> = [
            (Check::Type, "BadType"),
            (Check::UnusedParameter, "Unused"),
            (Check::Type, "Unused"),
            (Check::UndefinedTemplate, "Missing"),
            (Check::Arity, "Arity"),
            (Check::Cycle, "Ping"),
            (Check::Cycle, "Pong"),
            (Check::Type, "E1"),
            (Check::InconsistentUses, "E4"),
        ]
        .into_iter()
        .map(|(c, t)| (c, t.to_owned()))
        .collect();
        expected.sort();
        assert_eq!(found, expected);
    }

    #[test]
    fn what_tarka_reports_besides() {
        let text = "ex:Undeclared [ ] :: { ex:Named(?nope) } .
            ex:NonBlank [ ! ottr:IRI ?a ] :: { ottr:Triple(?a, rdfs:label, \"a\") } .
            ex:GivesBlank [ ] :: { ex:NonBlank(_:b) } .
            ex:Exp [ List<ottr:IRI> ?l, ottr:IRI ?i ] :: { cross | ex:Named(?i), ex:Named(++?l), cross | ex:Named(++?i) } .
            ex:Triples [ xsd:string ?s ] :: { ottr:Triple(\"lit\", ex:p, ex:o), ottr:Triple(ex:s, \"p\", ex:o), ottr:Triple(?s, ex:p, ex:o) } .
            ex:Odd [ schema:Thing ?x, ottr:IRI ?d = \"no\" ] :: { ottr:Triple(?d, ex:p, ?x) } .";
        let mut found = checks(text);
        found.sort();
        let mut expected: Vec<(Check, String)> = [
            (Check::UndeclaredVariable, "Undeclared"),
            (Check::NonBlank, "GivesBlank"),
            (Check::Expander, "Exp"),
            (Check::Expander, "Exp"),
            (Check::Expander, "Exp"),
            (Check::Triple, "Triples"),
            (Check::Triple, "Triples"),
            (Check::Triple, "Triples"),
            (Check::UnknownType, "Odd"),
            (Check::Default, "Odd"),
        ]
        .into_iter()
        .map(|(c, t)| (c, t.to_owned()))
        .collect();
        expected.sort();
        assert_eq!(found, expected, "{:#?}", lint_text(&format!("{PREFIXES}{text}")));
    }

    #[test]
    fn findings_say_where_and_what() {
        let findings = lint_text(&format!("{PREFIXES}ex:BadType [ xsd:integer ?n ] :: {{\n  ex:Named(?n) }} ."));
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert_eq!(
            findings[0].to_string(),
            "lib.stottr:6: error: ?n (xsd:integer) does not fit the type ottr:IRI of argument 1 of ex:Named [type]"
        );
    }
}
