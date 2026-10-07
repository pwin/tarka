//! RDF output with oxi-gen's duplicate handling.
//!
//! Triples are collected in a window and written when the window is full: each
//! row's triples by default (`window = 0`), or the last `window` distinct triples.
//! Duplicates inside a window are written once. For Turtle the window is sorted, so
//! triples about one subject come out together.

use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use flate2::Compression;
use flate2::write::GzEncoder;
use oxrdf::{GraphName, NamedNode, Quad, Triple};
use oxrdfio::{RdfFormat, RdfSerializer, WriterQuadSerializer};
use tarka_core::PrefixMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    NTriples,
    Turtle,
    /// N-Quads, every triple in one named graph.
    NQuads,
}

#[derive(Clone, Debug)]
pub struct OutputOptions {
    pub format: OutputFormat,
    /// Distinct triples to collect before writing; 0 writes after every row.
    pub window: usize,
    pub gzip: bool,
    /// The graph for N-Quads output.
    pub graph: Option<NamedNode>,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self { format: OutputFormat::Turtle, window: 0, gzip: false, graph: None }
    }
}

/// Somewhere to put triples, one row's worth at a time.
pub trait TripleSink {
    fn row(&mut self, triples: Vec<Triple>) -> io::Result<()>;

    fn finish(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Collects triples in memory (for tests and the library API).
impl TripleSink for Vec<Triple> {
    fn row(&mut self, triples: Vec<Triple>) -> io::Result<()> {
        self.extend(triples);
        Ok(())
    }
}

pub struct RdfWriter<W: Write> {
    serializer: Option<WriterQuadSerializer<W>>,
    /// The underlying writer, once finished.
    finished: Option<W>,
    options: OutputOptions,
    window: HashSet<Triple>,
    written: u64,
}

impl RdfWriter<Box<dyn Write>> {
    /// A writer to `path`, or to standard output for `None` or `-`.
    pub fn create(path: Option<&Path>, prefixes: &PrefixMap, options: OutputOptions) -> io::Result<Self> {
        let out: Box<dyn Write> = match path {
            Some(p) if p.as_os_str() != "-" => {
                let file = File::create(p)?;
                if options.gzip {
                    Box::new(BufWriter::new(GzEncoder::new(file, Compression::default())))
                } else {
                    Box::new(BufWriter::new(file))
                }
            }
            _ => Box::new(BufWriter::new(io::stdout().lock())),
        };
        Self::new(out, prefixes, options)
    }
}

impl<W: Write> RdfWriter<W> {
    pub fn new(out: W, prefixes: &PrefixMap, options: OutputOptions) -> io::Result<Self> {
        let mut serializer = RdfSerializer::from_format(match options.format {
            OutputFormat::NTriples => RdfFormat::NTriples,
            OutputFormat::Turtle => RdfFormat::Turtle,
            OutputFormat::NQuads => RdfFormat::NQuads,
        });
        if options.format == OutputFormat::Turtle {
            for (prefix, ns) in prefixes.iter() {
                serializer = serializer
                    .with_prefix(prefix, ns)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("prefix {prefix}: {e}")))?;
            }
        }
        if options.format == OutputFormat::NQuads && options.graph.is_none() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "N-Quads output needs a graph name"));
        }
        Ok(Self { serializer: Some(serializer.for_writer(out)), finished: None, options, window: HashSet::new(), written: 0 })
    }

    /// The number of triples written so far.
    pub fn written(&self) -> u64 {
        self.written
    }

    fn flush_window(&mut self) -> io::Result<()> {
        if self.window.is_empty() {
            return Ok(());
        }
        let mut triples: Vec<Triple> = self.window.drain().collect();
        if self.options.format == OutputFormat::Turtle {
            triples.sort_by_cached_key(|t| (t.subject.to_string(), t.predicate.to_string(), t.object.to_string()));
        }
        let serializer = self.serializer.as_mut().expect("writer used after finish");
        for t in &triples {
            match &self.options.graph {
                Some(g) if self.options.format == OutputFormat::NQuads => serializer.serialize_quad(&Quad::new(
                    t.subject.clone(),
                    t.predicate.clone(),
                    t.object.clone(),
                    GraphName::NamedNode(g.clone()),
                ))?,
                _ => serializer.serialize_triple(t)?,
            }
        }
        self.written += triples.len() as u64;
        Ok(())
    }

    /// Writes everything and returns the underlying writer (also after [`TripleSink::finish`]).
    pub fn into_inner(mut self) -> io::Result<W> {
        self.finish()?;
        Ok(self.finished.take().expect("a finished writer"))
    }
}

impl<W: Write> TripleSink for RdfWriter<W> {
    fn row(&mut self, triples: Vec<Triple>) -> io::Result<()> {
        self.window.extend(triples);
        if !self.window.is_empty() && self.window.len() >= self.options.window {
            self.flush_window()?;
        }
        Ok(())
    }

    fn finish(&mut self) -> io::Result<()> {
        self.flush_window()?;
        if let Some(serializer) = self.serializer.take() {
            let mut out = serializer.finish()?;
            out.flush()?;
            self.finished = Some(out);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxrdf::{Literal, NamedNode};

    fn t(s: &str, o: &str) -> Triple {
        Triple::new(
            NamedNode::new_unchecked(format!("http://ex/{s}")),
            NamedNode::new_unchecked("http://ex/p"),
            Literal::new_simple_literal(o),
        )
    }

    fn write(format: OutputFormat, window: usize, rows: Vec<Vec<Triple>>) -> String {
        let prefixes: PrefixMap = [("ex", "http://ex/")].into_iter().collect();
        let graph = (format == OutputFormat::NQuads).then(|| NamedNode::new_unchecked("http://ex/g"));
        let mut w = RdfWriter::new(Vec::new(), &prefixes, OutputOptions { format, window, gzip: false, graph }).unwrap();
        for r in rows {
            w.row(r).unwrap();
        }
        String::from_utf8(w.into_inner().unwrap()).unwrap()
    }

    #[test]
    fn duplicates_inside_a_window_are_written_once() {
        let rows = vec![vec![t("a", "1"), t("a", "1")], vec![t("a", "1")]];
        assert_eq!(write(OutputFormat::NTriples, 0, rows.clone()).lines().count(), 2);
        assert_eq!(write(OutputFormat::NTriples, 1000, rows).lines().count(), 1);
    }

    #[test]
    fn turtle_uses_prefixes_and_groups_subjects() {
        let out = write(OutputFormat::Turtle, 100, vec![vec![t("b", "1"), t("a", "1"), t("a", "2")]]);
        assert!(out.starts_with("@prefix ex: <http://ex/> ."), "{out}");
        assert!(out.contains("ex:a ex:p \"1\" , \"2\" ."), "{out}");
    }

    #[test]
    fn nquads_name_the_graph() {
        let out = write(OutputFormat::NQuads, 0, vec![vec![t("a", "1")]]);
        assert_eq!(out.trim_end(), "<http://ex/a> <http://ex/p> \"1\" <http://ex/g> .");
    }
}
