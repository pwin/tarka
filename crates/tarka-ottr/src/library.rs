//! A set of templates gathered from stOTTR documents.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use tarka_core::PrefixMap;

use crate::OttrError;
use crate::model::{Document, Kind, OTTR_TRIPLE, Template};
use crate::parser::{merge_prefixes, parse_stottr};

#[derive(Clone, Debug)]
pub struct Library {
    templates: HashMap<String, Template>,
    order: Vec<String>,
    pub prefixes: PrefixMap,
}

impl Default for Library {
    fn default() -> Self {
        let mut lib = Self { templates: HashMap::new(), order: Vec::new(), prefixes: PrefixMap::new() };
        lib.insert(Template::triple());
        lib
    }
}

impl Library {
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads `.stottr` files, and every `.stottr` file under the given directories.
    pub fn load(paths: &[impl AsRef<Path>]) -> Result<Self, OttrError> {
        let mut lib = Self::new();
        for path in paths {
            for file in stottr_files(path.as_ref())? {
                let text = fs::read_to_string(&file).map_err(|e| OttrError::Io(file.display().to_string(), e))?;
                lib.add(parse_stottr(&text, &file.display().to_string())?);
            }
        }
        Ok(lib)
    }

    /// Adds a document's templates and prefixes. A template with a pattern is kept
    /// over a later bare signature of the same IRI; earlier prefixes are kept.
    pub fn add(&mut self, doc: Document) {
        merge_prefixes(&mut self.prefixes, &doc.prefixes);
        for t in doc.templates {
            if matches!(self.templates.get(&t.iri), Some(prev) if prev.kind == Kind::Template && t.kind != Kind::Template) {
                continue;
            }
            self.insert(t);
        }
    }

    fn insert(&mut self, t: Template) {
        if !self.templates.contains_key(&t.iri) {
            self.order.push(t.iri.clone());
        }
        self.templates.insert(t.iri.clone(), t);
    }

    pub fn get(&self, iri: &str) -> Option<&Template> {
        self.templates.get(iri)
    }

    /// The templates in the order they were first added (`ottr:Triple` first).
    pub fn templates(&self) -> impl Iterator<Item = &Template> {
        self.order.iter().map(|i| &self.templates[i])
    }

    /// The IRI of a template named by a full IRI, `<IRI>` or a prefixed name.
    pub fn resolve(&self, name: &str) -> String {
        let name = name.trim_start_matches('<').trim_end_matches('>');
        if self.templates.contains_key(name) {
            return name.to_owned();
        }
        match self.prefixes.expand(name) {
            Some(iri) if !name.contains("://") => iri,
            _ => name.to_owned(),
        }
    }

    pub fn is_triple(iri: &str) -> bool {
        iri == OTTR_TRIPLE
    }
}

fn stottr_files(path: &Path) -> Result<Vec<PathBuf>, OttrError> {
    if !path.is_dir() {
        return Ok(vec![path.to_owned()]);
    }
    let mut out = Vec::new();
    let mut dirs = vec![path.to_owned()];
    while let Some(dir) = dirs.pop() {
        let entries = fs::read_dir(&dir).map_err(|e| OttrError::Io(dir.display().to_string(), e))?;
        for entry in entries {
            let p = entry.map_err(|e| OttrError::Io(dir.display().to_string(), e))?.path();
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|e| e == "stottr") {
                out.push(p);
            }
        }
    }
    out.sort();
    Ok(out)
}
