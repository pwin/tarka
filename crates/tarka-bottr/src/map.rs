//! Instance maps and argument maps, read from a bOTTR file (Turtle).

use std::path::{Path, PathBuf};

use oxrdf::vocab::rdf;
use oxrdf::{Graph, NamedNodeRef, NamedOrBlankNode, NamedOrBlankNodeRef, Term, TermRef, Triple};
use oxrdfio::{RdfFormat, RdfParser};
use tarka_core::{PrefixMap, TypeRef};

use crate::BottrError;

pub const OTTR: &str = "http://ns.ottr.xyz/0.4/";
const OTTR_NONE: &str = "http://ns.ottr.xyz/0.4/none";
/// Where a query or a source URL names the directory of the bOTTR file.
pub const THIS_DIR: &str = "@@THIS_DIR@@";

/// Where an instance map reads its rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// H2's `CSVREAD`, in tarka's subset of H2's SQL.
    H2,
    /// RDF files, queried with SPARQL.
    RdfFiles(Vec<PathBuf>),
    /// A source tarka does not read (JDBC, a SPARQL endpoint), by class.
    Unsupported(String),
}

/// An `ottr:InstanceMap`: a query whose rows are instances of a template.
#[derive(Clone, Debug)]
pub struct InstanceMap {
    /// The bOTTR file, and the map's position in it (from 1), for messages.
    pub file: String,
    pub index: usize,
    pub template: String,
    pub source: Source,
    pub query: String,
    /// One per column of the query; the defaults if the map gives none.
    pub arguments: Option<Vec<ArgumentMap>>,
    /// The bOTTR file's prefixes: IRI cells are expanded with them.
    pub prefixes: PrefixMap,
}

/// How one column's values become arguments.
#[derive(Clone, Debug)]
pub struct ArgumentMap {
    pub ty: Option<TypeRef>,
    pub language_tag: Option<String>,
    pub language_tag_sep: Option<String>,
    pub datatype_sep: Option<String>,
    /// The argument for a missing value; `None` is `ottr:none`.
    pub null_value: Option<Term>,
    pub labelled_blank_prefix: String,
    pub fresh_blank: Vec<String>,
    pub boolean_true: Vec<String>,
    pub boolean_false: Vec<String>,
    pub list_sep: String,
    pub list_start: char,
    pub list_end: char,
    /// Values and what they become (`None`: `ottr:none`).
    pub translation: Vec<(Term, Option<Term>)>,
}

impl Default for ArgumentMap {
    fn default() -> Self {
        Self {
            ty: None,
            language_tag: None,
            language_tag_sep: None,
            datatype_sep: None,
            null_value: None,
            labelled_blank_prefix: "_:".into(),
            fresh_blank: Vec::new(),
            boolean_true: Vec::new(),
            boolean_false: Vec::new(),
            list_sep: ",".into(),
            list_start: '(',
            list_end: ')',
            translation: Vec::new(),
        }
    }
}

/// The instance maps of a bOTTR file.
pub fn load(path: &Path) -> Result<Vec<InstanceMap>, BottrError> {
    let file = path.display().to_string();
    let text = std::fs::read_to_string(path).map_err(|e| BottrError::Io(file.clone(), e))?;
    let dir = std::path::absolute(path).ok().and_then(|p| p.parent().map(Path::to_owned)).unwrap_or_default();
    parse(&text, &file, &dir)
}

/// The instance maps of a bOTTR document; `dir` stands for `@@THIS_DIR@@` and is where
/// relative source files are found.
pub fn parse(text: &str, file: &str, dir: &Path) -> Result<Vec<InstanceMap>, BottrError> {
    let base = url_of(&dir.join("map.ttl"));
    let mut parser = RdfParser::from_format(RdfFormat::Turtle).with_base_iri(&base).expect("a file URL").for_slice(text.as_bytes());
    let mut graph = Graph::new();
    // the instance maps in the order the document gives them
    let mut maps: Vec<NamedOrBlankNode> = Vec::new();
    let instance_map = ottr("InstanceMap");
    for quad in parser.by_ref() {
        let quad = quad.map_err(|e| BottrError::Map { file: file.into(), message: e.to_string() })?;
        if quad.predicate == rdf::TYPE
            && matches!(&quad.object, Term::NamedNode(n) if n.as_str() == instance_map)
            && !maps.contains(&quad.subject)
        {
            maps.push(quad.subject.clone());
        }
        graph.insert(&Triple::new(quad.subject, quad.predicate, quad.object));
    }
    let prefixes: PrefixMap = parser.prefixes().map(|(p, ns)| (p.to_owned(), ns.to_owned())).collect();
    let g = Doc { graph: &graph, file, dir };
    let mut out = Vec::new();
    for (i, node) in maps.iter().map(NamedOrBlankNode::as_ref).enumerate() {
        let index = i + 1;
        let err = |message: String| BottrError::Map { file: file.into(), message: format!("instance map {index}: {message}") };
        let template = match g.object(node, "template") {
            Some(Term::NamedNode(n)) => n.into_string(),
            _ => return Err(err("ottr:template must name a template".into())),
        };
        let query = match g.object(node, "query") {
            Some(Term::Literal(l)) => l.value().replace(THIS_DIR, &slashes(dir)),
            _ => return Err(err("ottr:query must be a string".into())),
        };
        let source = match g.object(node, "source") {
            Some(s) => g.source(&s).map_err(err)?,
            None => return Err(err("ottr:source is missing".into())),
        };
        let arguments = match g.object(node, "argumentMaps") {
            None => None,
            Some(list) => Some(g.list(&list).map_err(err)?.iter().map(|a| g.argument_map(a)).collect::<Result<Vec<_>, _>>().map_err(err)?),
        };
        out.push(InstanceMap { file: file.into(), index, template, source, query, arguments, prefixes: prefixes.clone() });
    }
    Ok(out)
}

struct Doc<'a> {
    graph: &'a Graph,
    file: &'a str,
    dir: &'a Path,
}

fn ottr(local: &str) -> String {
    format!("{OTTR}{local}")
}

impl Doc<'_> {
    fn object(&self, s: NamedOrBlankNodeRef<'_>, local: &str) -> Option<Term> {
        let p = ottr(local);
        self.graph.object_for_subject_predicate(s, NamedNodeRef::new_unchecked(&p)).map(TermRef::into_owned)
    }

    fn subject<'t>(t: &'t Term) -> Option<NamedOrBlankNodeRef<'t>> {
        match t {
            Term::NamedNode(n) => Some(n.into()),
            Term::BlankNode(b) => Some(b.into()),
            _ => None,
        }
    }

    /// The items of an RDF list.
    fn list(&self, head: &Term) -> Result<Vec<Term>, String> {
        let mut out = Vec::new();
        let mut node = head.clone();
        loop {
            if node == Term::NamedNode(rdf::NIL.into_owned()) {
                return Ok(out);
            }
            let s = Self::subject(&node).ok_or("a list ends in something other than rdf:nil")?;
            let first = self.graph.object_for_subject_predicate(s, rdf::FIRST).ok_or("a list item without rdf:first")?;
            out.push(first.into_owned());
            node = self.graph.object_for_subject_predicate(s, rdf::REST).ok_or("a list item without rdf:rest")?.into_owned();
            if out.len() > 100_000 {
                return Err("a list that does not end".into());
            }
        }
    }

    fn source(&self, node: &Term) -> Result<Source, String> {
        let s = Self::subject(node).ok_or("ottr:source must be a node")?;
        let classes: Vec<String> = self.graph.objects_for_subject_predicate(s, rdf::TYPE).map(|t| t.to_string()).collect();
        let is = |local: &str| classes.iter().any(|c| *c == format!("<{OTTR}{local}>"));
        if is("H2Source") {
            if self.object(s, "sourceURL").is_some() {
                return Err("an H2 source with an ottr:sourceURL (a database file) is not supported: tarka reads CSVREAD queries".into());
            }
            return Ok(Source::H2);
        }
        if is("RDFFileSource") {
            let urls: Vec<PathBuf> = self
                .graph
                .objects_for_subject_predicate(s, NamedNodeRef::new_unchecked(&ottr("sourceURL")))
                .map(|t| match t {
                    TermRef::Literal(l) => Ok(self.file_path(l.value())),
                    TermRef::NamedNode(n) => Ok(self.file_path(n.as_str())),
                    _ => Err("ottr:sourceURL must be a string or an IRI"),
                })
                .collect::<Result<_, _>>()?;
            if urls.is_empty() {
                return Err("an RDF file source needs an ottr:sourceURL".into());
            }
            return Ok(Source::RdfFiles(urls));
        }
        Ok(Source::Unsupported(classes.into_iter().next().unwrap_or_else(|| "an untyped source".into())))
    }

    /// A source URL as a file path: `file:` IRIs, and paths relative to the bOTTR file.
    fn file_path(&self, url: &str) -> PathBuf {
        let url = url.replace(THIS_DIR, &slashes(self.dir));
        let path = url.strip_prefix("file:///").or_else(|| url.strip_prefix("file:")).unwrap_or(&url);
        let path = percent_decode(path);
        let p = PathBuf::from(&path);
        if p.is_absolute() || path.starts_with('/') { p } else { self.dir.join(p) }
    }

    fn string(&self, s: NamedOrBlankNodeRef, local: &str) -> Result<Option<String>, String> {
        match self.object(s, local) {
            None => Ok(None),
            Some(Term::Literal(l)) => Ok(Some(l.value().to_owned())),
            Some(other) => Err(format!("ottr:{local} must be a string, not {other}")),
        }
    }

    fn strings(&self, s: NamedOrBlankNodeRef, local: &str) -> Result<Vec<String>, String> {
        match self.object(s, local) {
            None => Ok(Vec::new()),
            Some(list) => self
                .list(&list)?
                .into_iter()
                .map(|t| match t {
                    Term::Literal(l) => Ok(l.value().to_owned()),
                    other => Err(format!("ottr:{local} lists strings, not {other}")),
                })
                .collect(),
        }
    }

    fn char(&self, s: NamedOrBlankNodeRef, local: &str, default: char) -> Result<char, String> {
        match self.string(s, local)? {
            None => Ok(default),
            Some(v) => {
                let mut chars = v.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => Ok(c),
                    _ => Err(format!("ottr:{local} must be one character, not {v:?}")),
                }
            }
        }
    }

    fn argument_map(&self, node: &Term) -> Result<ArgumentMap, String> {
        let Some(s) = Self::subject(node) else { return Err(format!("an argument map must be a node, not {node}")) };
        if self.object(s, "translationSettings").is_some() {
            eprintln!(
                "{}: ottr:translationSettings is deprecated and ignored (Lutra ignores it too): give the settings on the argument map",
                self.file
            );
        }
        let ty = match self.object(s, "type") {
            None => None,
            Some(t) => Some(self.type_of(&t)?),
        };
        let mut map = ArgumentMap {
            ty,
            language_tag: self.string(s, "languageTag")?,
            language_tag_sep: self.string(s, "languageTagSep")?,
            datatype_sep: self.string(s, "datatypeSep")?,
            null_value: match self.object(s, "nullValue") {
                Some(Term::NamedNode(n)) if n.as_str() == OTTR_NONE => None,
                Some(Term::BlankNode(_)) => return Err("ottr:nullValue cannot be a blank node".into()),
                other => other,
            },
            ..ArgumentMap::default()
        };
        if let Some(prefix) = self.string(s, "labelledBlankPrefix")? {
            map.labelled_blank_prefix = prefix;
        }
        map.fresh_blank = self.strings(s, "blankNodeFresh")?;
        map.boolean_true = self.strings(s, "booleanTrue")?;
        map.boolean_false = self.strings(s, "booleanFalse")?;
        if let Some(sep) = self.string(s, "listSep")? {
            map.list_sep = sep;
        }
        map.list_start = self.char(s, "listStart", '(')?;
        map.list_end = self.char(s, "listEnd", ')')?;
        if let Some(table) = self.object(s, "translationTable") {
            let table = Self::subject(&table).ok_or("ottr:translationTable must be a node")?;
            for entry in self.graph.objects_for_subject_predicate(table, NamedNodeRef::new_unchecked(&ottr("entry"))) {
                let entry = entry.into_owned();
                let e = Self::subject(&entry).ok_or("a translation table entry must be a node")?;
                let input = self.object(e, "inValue").ok_or("a translation table entry without ottr:inValue")?;
                let output = match self.object(e, "outValue").ok_or("a translation table entry without ottr:outValue")? {
                    Term::NamedNode(n) if n.as_str() == OTTR_NONE => None,
                    other => Some(other),
                };
                map.translation.push((input, output));
            }
        }
        // the combinations Lutra refuses
        let given = [
            (map.ty.is_some(), "a type"),
            (map.language_tag.is_some(), "a language tag"),
            (map.language_tag_sep.is_some(), "a language tag separator"),
            (map.datatype_sep.is_some(), "a datatype separator"),
        ];
        let set: Vec<&str> = given.iter().filter(|(g, _)| *g).map(|(_, w)| *w).collect();
        if set.len() > 1 {
            return Err(format!("an argument map cannot have both {} and {}", set[0], set[1]));
        }
        if let Some(tag) = &map.language_tag
            && oxrdf::Literal::new_language_tagged_literal("x", tag).is_err()
        {
            return Err(format!("ottr:languageTag {tag:?} is not a language tag"));
        }
        Ok(map)
    }

    /// A type in WOTTR's syntax: an IRI, or a list `(rdf:List T)`, `(ottr:NEList T)` or
    /// `(ottr:LUB T)`, nested or flat (`(rdf:List rdf:List xsd:string)`).
    fn type_of(&self, t: &Term) -> Result<TypeRef, String> {
        match t {
            Term::NamedNode(n) => Ok(TypeRef::Basic(n.clone())),
            Term::BlankNode(_) => {
                let items = self.list(t)?;
                fn build(doc: &Doc, items: &[Term]) -> Result<TypeRef, String> {
                    let Some((first, rest)) = items.split_first() else { return Err("an empty type".into()) };
                    let wrap = |inner: TypeRef| -> Result<TypeRef, String> {
                        match first {
                            Term::NamedNode(n) if n.as_str() == rdf::LIST.as_str() => Ok(TypeRef::List(Box::new(inner))),
                            Term::NamedNode(n) if n.as_str() == ottr("NEList") => Ok(TypeRef::NeList(Box::new(inner))),
                            Term::NamedNode(n) if n.as_str() == ottr("LUB") => Ok(TypeRef::Lub(Box::new(inner))),
                            other => Err(format!("{other} is not a type constructor (rdf:List, ottr:NEList or ottr:LUB)")),
                        }
                    };
                    match rest {
                        [] => match first {
                            Term::NamedNode(n) => Ok(TypeRef::Basic(n.clone())),
                            other => Err(format!("{other} is not a type")),
                        },
                        [inner @ Term::BlankNode(_)] => wrap(doc.type_of(inner)?),
                        _ => wrap(build(doc, rest)?),
                    }
                }
                build(self, &items)
            }
            other => Err(format!("{other} is not a type")),
        }
    }
}

fn slashes(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

/// A `file:` URL for a path.
fn url_of(p: &Path) -> String {
    let s = slashes(p);
    let s = s.replace(' ', "%20");
    if s.starts_with('/') { format!("file://{s}") } else { format!("file:///{s}") }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
