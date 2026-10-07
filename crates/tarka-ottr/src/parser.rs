//! The stOTTR parser.

use oxrdf::vocab::xsd;
use oxrdf::{Literal, NamedNode};
use tarka_core::{Expander, PrefixMap, TypeRef};

use crate::OttrError;
use crate::lexer::{Tok, Token, tokenize};
use crate::model::{Document, Instance, Kind, OTerm, Param, Template};

/// Parses a stOTTR document. `source` names it in error messages.
pub fn parse_stottr(text: &str, source: &str) -> Result<Document, OttrError> {
    let tokens = tokenize(text).map_err(|e| OttrError::Syntax { file: source.into(), line: e.line, message: e.message })?;
    let mut p = Parser { toks: tokens, pos: 0, doc: Document::default(), base: None, anon: 0, source: source.into() };
    while p.pos < p.toks.len() {
        p.statement()?;
    }
    Ok(p.doc)
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    doc: Document,
    base: Option<String>,
    anon: usize,
    source: String,
}

impl Parser {
    fn peek(&self, k: usize) -> Option<&Tok> {
        self.toks.get(self.pos + k).map(|t| &t.tok)
    }

    fn line(&self) -> usize {
        self.toks.get(self.pos).or_else(|| self.toks.last()).map_or(0, |t| t.line)
    }

    fn err<T>(&self, message: impl Into<String>) -> Result<T, OttrError> {
        Err(OttrError::Syntax { file: self.source.clone(), line: self.line(), message: message.into() })
    }

    fn next(&mut self) -> Result<Tok, OttrError> {
        match self.toks.get(self.pos) {
            Some(t) => {
                self.pos += 1;
                Ok(t.tok.clone())
            }
            None => self.err("unexpected end of input"),
        }
    }

    fn at(&self, p: &str) -> bool {
        matches!(self.peek(0), Some(Tok::Punct(q)) if *q == p)
    }

    fn at_name(&self, n: &str) -> bool {
        matches!(self.peek(0), Some(Tok::Name(m)) if m == n)
    }

    fn expect(&mut self, p: &str) -> Result<(), OttrError> {
        if self.at(p) {
            self.pos += 1;
            Ok(())
        } else {
            let found = self.peek(0).map_or("end of input".to_owned(), |t| format!("{t}"));
            self.err(format!("expected '{p}' but found {found}"))
        }
    }

    fn statement(&mut self) -> Result<(), OttrError> {
        match self.peek(0) {
            Some(Tok::Directive(d)) => {
                let d = *d;
                self.pos += 1;
                self.directive(d == "@prefix")?;
                return self.expect(".");
            }
            Some(Tok::Name(n)) if n.eq_ignore_ascii_case("PREFIX") || n.eq_ignore_ascii_case("BASE") => {
                let prefix = n.eq_ignore_ascii_case("PREFIX");
                self.pos += 1;
                return self.directive(prefix);
            }
            _ => {}
        }
        if self.expander_ahead() {
            let inst = self.instance()?;
            self.doc.instances.push(inst);
        } else {
            let line = self.line();
            let name = self.iri()?;
            if self.at("[") {
                let mut t = self.signature(name, line)?;
                if self.at("::") {
                    self.pos += 1;
                    if self.at_name("BASE") {
                        self.pos += 1;
                        t.kind = Kind::Base;
                    } else {
                        t.body = self.pattern()?;
                        t.kind = Kind::Template;
                    }
                }
                self.doc.templates.push(t);
            } else {
                let inst = self.instance_after(name, None, line)?;
                self.doc.instances.push(inst);
            }
        }
        self.expect(".")
    }

    fn directive(&mut self, prefix: bool) -> Result<(), OttrError> {
        if prefix {
            let Tok::PName(p, local) = self.next()? else { return self.err("expected a prefix name such as ex:") };
            if !local.is_empty() {
                return self.err(format!("bad prefix declaration {p}:{local}"));
            }
            let Tok::Iri(ns) = self.next()? else { return self.err("expected an IRI") };
            let ns = self.resolve(&ns);
            self.doc.prefixes.insert(p, ns);
        } else {
            let Tok::Iri(b) = self.next()? else { return self.err("expected an IRI") };
            self.base = Some(self.resolve(&b));
        }
        Ok(())
    }

    fn expander_ahead(&self) -> bool {
        matches!(self.peek(0), Some(Tok::Name(n)) if ["cross", "zipMin", "zipMax"].contains(&n.as_str()))
            && matches!(self.peek(1), Some(Tok::Punct("|")))
    }

    fn signature(&mut self, iri: String, line: usize) -> Result<Template, OttrError> {
        self.expect("[")?;
        let mut params = Vec::new();
        while !self.at("]") {
            params.push(self.param()?);
            if self.at(",") {
                self.pos += 1;
            }
        }
        self.expect("]")?;
        let mut annotations = Vec::new();
        while self.at("@@") {
            self.pos += 1;
            annotations.push(self.instance()?);
            if self.at(",") {
                self.pos += 1;
            }
        }
        Ok(Template { iri, params, body: Vec::new(), annotations, kind: Kind::Signature, line })
    }

    fn param(&mut self) -> Result<Param, OttrError> {
        let mut p = Param { name: String::new(), ty: None, optional: false, non_blank: false, default: None };
        loop {
            if self.at("?") {
                p.optional = true;
            } else if self.at("!") {
                p.non_blank = true;
            } else {
                break;
            }
            self.pos += 1;
        }
        if !matches!(self.peek(0), Some(Tok::Var(_))) {
            p.ty = Some(self.ty()?);
        }
        let Tok::Var(name) = self.next()? else { return self.err("expected a parameter variable") };
        p.name = name;
        if self.at("=") {
            self.pos += 1;
            p.default = Some(self.term()?);
        }
        Ok(p)
    }

    fn ty(&mut self) -> Result<TypeRef, OttrError> {
        if let Some(Tok::TypeOpen(open)) = self.peek(0) {
            let open = *open;
            self.pos += 1;
            let inner = Box::new(self.ty()?);
            self.expect(">")?;
            return Ok(match open {
                "LUB<" => TypeRef::Lub(inner),
                "NEList<" => TypeRef::NeList(inner),
                _ => TypeRef::List(inner),
            });
        }
        Ok(TypeRef::Basic(NamedNode::new_unchecked(self.iri()?)))
    }

    fn pattern(&mut self) -> Result<Vec<Instance>, OttrError> {
        self.expect("{")?;
        let mut body = Vec::new();
        while !self.at("}") {
            body.push(self.instance()?);
            if self.at(",") {
                self.pos += 1;
            }
        }
        self.expect("}")?;
        Ok(body)
    }

    fn instance(&mut self) -> Result<Instance, OttrError> {
        let line = self.line();
        let expander = if self.expander_ahead() {
            let Tok::Name(n) = self.next()? else { unreachable!() };
            self.pos += 1; // '|'
            Some(match n.as_str() {
                "cross" => Expander::Cross,
                "zipMin" => Expander::ZipMin,
                _ => Expander::ZipMax,
            })
        } else {
            None
        };
        let template = self.iri()?;
        self.instance_after(template, expander, line)
    }

    fn instance_after(&mut self, template: String, expander: Option<Expander>, line: usize) -> Result<Instance, OttrError> {
        self.expect("(")?;
        let (mut args, mut expand) = (Vec::new(), Vec::new());
        while !self.at(")") {
            let flagged = self.at("++");
            if flagged {
                self.pos += 1;
            }
            args.push(self.term()?);
            expand.push(flagged);
            if self.at(",") {
                self.pos += 1;
            }
        }
        self.expect(")")?;
        Ok(Instance { template, args, expand, expander, line })
    }

    fn iri(&mut self) -> Result<String, OttrError> {
        match self.next()? {
            Tok::Iri(i) => Ok(self.resolve(&i)),
            Tok::PName(p, local) => match self.doc.prefixes.get(&p) {
                Some(ns) => Ok(format!("{ns}{local}")),
                None => self.err(format!("undeclared prefix '{p}:'")),
            },
            other => self.err(format!("expected an IRI but found {other}")),
        }
    }

    fn term(&mut self) -> Result<OTerm, OttrError> {
        match self.peek(0) {
            Some(Tok::Iri(_) | Tok::PName(..)) => return self.iri().map(OTerm::Iri),
            None => return self.err("unexpected end of input"),
            _ => {}
        }
        match self.next()? {
            Tok::Var(v) => Ok(OTerm::Var(v)),
            Tok::BNode(b) => Ok(OTerm::BNode(b)),
            Tok::Str(s) => match self.peek(0) {
                Some(Tok::LangTag(_)) => {
                    let Tok::LangTag(tag) = self.next()? else { unreachable!() };
                    match Literal::new_language_tagged_literal(s, &tag) {
                        Ok(l) => Ok(OTerm::Literal(l)),
                        Err(_) => self.err(format!("invalid language tag @{tag}")),
                    }
                }
                Some(Tok::DType) => {
                    self.pos += 1;
                    let dt = self.iri()?;
                    Ok(OTerm::Literal(Literal::new_typed_literal(s, NamedNode::new_unchecked(dt))))
                }
                _ => Ok(OTerm::Literal(Literal::new_simple_literal(s))),
            },
            Tok::Number(n, kind) => {
                let dt = match kind {
                    "integer" => xsd::INTEGER,
                    "decimal" => xsd::DECIMAL,
                    _ => xsd::DOUBLE,
                };
                Ok(OTerm::Literal(Literal::new_typed_literal(n, dt)))
            }
            Tok::Name(n) if n == "true" || n == "false" => Ok(OTerm::Literal(Literal::new_typed_literal(n, xsd::BOOLEAN))),
            Tok::Name(n) if n == "none" => Ok(OTerm::None),
            Tok::Punct("[") => {
                self.expect("]")?;
                self.anon += 1;
                Ok(OTerm::BNode(format!("anon{}", self.anon)))
            }
            Tok::Punct("(") => {
                let mut items = Vec::new();
                while !self.at(")") {
                    items.push(self.term()?);
                    if self.at(",") {
                        self.pos += 1;
                    }
                }
                self.expect(")")?;
                Ok(OTerm::List(items))
            }
            other => self.err(format!("unexpected {other}")),
        }
    }

    fn resolve(&self, iri: &str) -> String {
        match &self.base {
            Some(b) if !iri.split('/').next().unwrap_or("").contains(':') => {
                oxiri::Iri::parse(b.clone()).ok().and_then(|b| b.resolve(iri).ok()).map_or_else(|| iri.to_owned(), oxiri::Iri::into_inner)
            }
            _ => iri.to_owned(),
        }
    }
}

/// Merges prefixes into a map, keeping existing declarations.
pub(crate) fn merge_prefixes(into: &mut PrefixMap, from: &PrefixMap) {
    for (p, ns) in from.iter() {
        into.insert_if_absent(p, ns);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_and_instances() {
        let doc = parse_stottr(
            r#"@prefix ex: <http://ex/> . PREFIX ottr: <http://ns.ottr.xyz/0.4/>
            @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
            ex:T [ !ottr:IRI ?x, ? NEList<xsd:int> ?l, ?y = "d"@en ] @@ ex:Note("n")
              :: { cross | ex:U(?x, ++?l), ottr:Triple(?x, ex:p, [ ]), ottr:Triple(?x, ex:q, (1, 2.0, 3e0, true, none)) } .
            ex:B [ ?a ] :: BASE .
            ex:S [ ?a ] .
            ex:T(ex:a, (1, 2), none) .
            zipMax | ex:T(_:b, ++((1), (2)), "x"^^xsd:string) ."#,
            "test",
        )
        .unwrap();
        assert_eq!(doc.templates.len(), 3);
        let t = &doc.templates[0];
        assert_eq!(t.iri, "http://ex/T");
        assert_eq!((t.params[0].non_blank, t.params[0].optional), (true, false));
        assert!(matches!(t.params[1].ty, Some(TypeRef::NeList(_))));
        assert_eq!(t.params[2].default, Some(OTerm::Literal(Literal::new_language_tagged_literal_unchecked("d", "en"))));
        assert_eq!(t.annotations.len(), 1);
        assert_eq!(t.body[0].expander, Some(Expander::Cross));
        assert_eq!(t.body[0].expand, [false, true]);
        assert!(matches!(&t.body[1].args[2], OTerm::BNode(_)));
        assert_eq!((doc.templates[1].kind, doc.templates[2].kind), (Kind::Base, Kind::Signature));
        assert_eq!(doc.instances.len(), 2);
        assert_eq!(doc.instances[1].expander, Some(Expander::ZipMax));
        assert_eq!(doc.instances[0].args[2], OTerm::None);
    }

    #[test]
    fn errors_name_the_line() {
        let err = parse_stottr("@prefix ex: <http://ex/> .\nex:T [ ?x ] :: { nope:U(?x) } .", "lib.stottr").unwrap_err();
        assert_eq!(err.to_string(), "lib.stottr:2: undeclared prefix 'nope:'");
    }

    /// RDF 1.2's base directions are not language tags, whichever way oxrdf was built.
    #[test]
    fn directional_literals_are_refused() {
        let text = "@prefix ex: <http://ex/> . @prefix ottr: <http://ns.ottr.xyz/0.4/> .\nex:T [ ?x ] :: { ottr:Triple(?x, ex:p, \"hi\"@en--ltr) } .";
        let err = parse_stottr(text, "lib.stottr").unwrap_err();
        assert_eq!(err.to_string(), "lib.stottr:2: invalid language tag @en--ltr");
    }
}
