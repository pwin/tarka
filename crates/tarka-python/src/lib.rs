//! The `tarka` Python module.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use oxrdf::NamedNode;
use pyo3::exceptions::{PyIOError, PyValueError};
use pyo3::prelude::*;
use pyo3_polars::PyDataFrame;
use tarka::io::{CsvOptions, CsvSource, OutputFormat, OutputOptions, RdfWriter, Split};
use tarka::ottr::{CompileOptions, Library, OttrError};
use tarka::{Plan, RunOptions};
use tarka_polars::FrameOptions;

fn value_error(e: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(e.to_string())
}

/// An OTTR error: OSError when a file cannot be read, else ValueError.
fn ottr_error(e: OttrError) -> PyErr {
    match e {
        OttrError::Io(..) => PyIOError::new_err(e.to_string()),
        e => value_error(e),
    }
}

/// A compiled mapping: a TARQL query or OTTR templates.
#[pyclass(module = "tarka", frozen)]
struct Mapping {
    plan: Plan,
}

#[pymethods]
impl Mapping {
    /// A mapping from the text of a TARQL query.
    #[staticmethod]
    #[pyo3(signature = (query, name = "query"))]
    fn tarql(query: &str, name: &str) -> PyResult<Self> {
        Ok(Self { plan: tarka::tarql::parse_tarql(query, name).map_err(value_error)? })
    }

    /// A mapping from a TARQL query file.
    #[staticmethod]
    fn tarql_file(path: PathBuf) -> PyResult<Self> {
        let text = std::fs::read_to_string(&path).map_err(|e| PyIOError::new_err(format!("{}: {e}", path.display())))?;
        let name = path.file_stem().map_or_else(|| "query".into(), |s| s.to_string_lossy().into_owned());
        Self::tarql(&text, &name)
    }

    /// A mapping from OTTR templates: `library` is files or directories of stOTTR, and
    /// `templates` the root templates to instantiate for every row. `lists` gives the
    /// separator of text cells for list-typed parameters (list columns need none).
    #[staticmethod]
    #[pyo3(signature = (library, templates, lists = None))]
    fn ottr(library: Vec<PathBuf>, templates: Vec<String>, lists: Option<HashMap<String, String>>) -> PyResult<Self> {
        let lib = Library::load(&library).map_err(ottr_error)?;
        let roots: Vec<&str> = templates.iter().map(String::as_str).collect();
        let options = CompileOptions { given: false, lists: lists.unwrap_or_default() };
        Ok(Self { plan: tarka::ottr::compile_many(&lib, &roots, &options).map_err(ottr_error)? })
    }

    /// The plan's name (the query or the root templates).
    #[getter]
    fn name(&self) -> &str {
        &self.plan.name
    }

    /// The prefixes, for Turtle output.
    #[getter]
    fn prefixes(&self) -> Vec<(String, String)> {
        self.plan.prefixes.iter().map(|(p, n)| (p.to_owned(), n.to_owned())).collect()
    }

    /// The RDF of a Polars data frame, as a data frame of N-Triples terms with the
    /// columns `subject`, `predicate` and `object` (no duplicates).
    #[pyo3(signature = (df, bind_empty_strings = false))]
    fn triplify(&self, py: Python<'_>, df: PyDataFrame, bind_empty_strings: bool) -> PyResult<PyDataFrame> {
        let options = FrameOptions { bind_empty_strings };
        let out = py.detach(|| tarka_polars::triplify(&self.plan, &df.0, &options)).map_err(value_error)?;
        Ok(PyDataFrame(out))
    }

    /// Writes the RDF of a Polars data frame to `path`, or returns it as text when
    /// `path` is None. `format` is "turtle", "ntriples" or "nquads" (with `graph`).
    #[pyo3(signature = (df, path = None, format = "turtle", graph = None, dedup = 0, bind_empty_strings = false))]
    #[allow(clippy::too_many_arguments)]
    fn write(
        &self,
        py: Python<'_>,
        df: PyDataFrame,
        path: Option<PathBuf>,
        format: &str,
        graph: Option<&str>,
        dedup: usize,
        bind_empty_strings: bool,
    ) -> PyResult<Option<String>> {
        let output = output_options(format, graph, dedup)?;
        let frame = FrameOptions { bind_empty_strings };
        let plan = &self.plan;
        py.detach(|| {
            write_to(path.as_deref(), plan, output, |sink| {
                tarka_polars::run_frame(plan, &df.0, &frame, sink, &RunOptions::default()).map(|_| ()).map_err(value_error)
            })
        })
    }

    /// Maps a CSV file (read the way oxi-gen reads it) and writes the RDF to `output`,
    /// or returns it as text when `output` is None. Each `split` (column, new column,
    /// separator) repeats a row once per part of the column, as `--split` does.
    #[pyo3(signature = (input, output = None, format = "turtle", graph = None, dedup = 0, delimiter = ",", quote = "\"",
                        escape = "\\", has_header = true, bind_empty_strings = false, split = None))]
    #[allow(clippy::too_many_arguments)]
    fn run_csv(
        &self,
        py: Python<'_>,
        input: PathBuf,
        output: Option<PathBuf>,
        format: &str,
        graph: Option<&str>,
        dedup: usize,
        delimiter: &str,
        quote: &str,
        escape: &str,
        has_header: bool,
        bind_empty_strings: bool,
        split: Option<Vec<(String, String, String)>>,
    ) -> PyResult<Option<String>> {
        let byte = |s: &str, what: &str| match s.as_bytes() {
            [b] => Ok(*b),
            _ => Err(value_error(format!("the {what} must be one ASCII character"))),
        };
        let csv = CsvOptions {
            delimiter: byte(delimiter, "delimiter")?,
            quote: byte(quote, "quote")?,
            escape: Some(byte(escape, "escape character")?),
            has_headers: has_header,
            bind_empty_strings,
            splits: split.unwrap_or_default().into_iter().map(|(column, name, separator)| Split { column, name, separator }).collect(),
            ..CsvOptions::default()
        };
        let output_opts = output_options(format, graph, dedup)?;
        let plan = &self.plan;
        py.detach(|| {
            let file = std::fs::File::open(&input).map_err(|e| PyIOError::new_err(format!("{}: {e}", input.display())))?;
            let source = CsvSource::new(std::io::BufReader::new(file), csv).map_err(value_error)?;
            let columns = source.columns().to_vec();
            write_to(output.as_deref(), plan, output_opts, |sink| {
                tarka::run(plan, &columns, source, sink, &RunOptions::default()).map(|_| ()).map_err(value_error)
            })
        })
    }

    fn __repr__(&self) -> String {
        format!("<tarka.Mapping {}>", self.plan.name)
    }
}

/// Expands OTTR instances (stOTTR files) with a library, as Lutra does; writes to
/// `output`, or returns the RDF as text when `output` is None.
#[pyfunction]
#[pyo3(signature = (library, instances, output = None, format = "turtle", graph = None))]
fn expand(
    py: Python<'_>,
    library: Vec<PathBuf>,
    instances: Vec<PathBuf>,
    output: Option<PathBuf>,
    format: &str,
    graph: Option<&str>,
) -> PyResult<Option<String>> {
    let options = output_options(format, graph, 0)?;
    py.detach(|| {
        let mut lib = Library::load(&library).map_err(ottr_error)?;
        let mut all = Vec::new();
        for path in &instances {
            let text = std::fs::read_to_string(path).map_err(|e| PyIOError::new_err(format!("{}: {e}", path.display())))?;
            let doc = tarka::ottr::parse_stottr(&text, &path.display().to_string()).map_err(ottr_error)?;
            for (p, ns) in doc.prefixes.iter() {
                lib.prefixes.insert_if_absent(p, ns);
            }
            all.extend(doc.instances);
        }
        let mut plan = Plan::new("instances", tarka::core::Lifting::Given);
        plan.prefixes = lib.prefixes.clone();
        write_to(output.as_deref(), &plan, options, |sink| tarka::expand_instances(&lib, &all, sink).map(|_| ()).map_err(value_error))
    })
}

fn output_options(format: &str, graph: Option<&str>, dedup: usize) -> PyResult<OutputOptions> {
    let format = match format {
        "turtle" | "ttl" => OutputFormat::Turtle,
        "ntriples" | "nt" => OutputFormat::NTriples,
        "nquads" | "nq" => OutputFormat::NQuads,
        other => return Err(value_error(format!("unknown format {other:?}: use turtle, ntriples or nquads"))),
    };
    let graph = graph.map(NamedNode::new).transpose().map_err(value_error)?;
    if format == OutputFormat::NQuads && graph.is_none() {
        return Err(value_error("N-Quads output needs a graph"));
    }
    Ok(OutputOptions { format, window: dedup, gzip: false, graph })
}

/// Runs `body` with a writer to `path`, or to a buffer whose text is returned.
fn write_to(
    path: Option<&Path>,
    plan: &Plan,
    options: OutputOptions,
    body: impl FnOnce(&mut dyn tarka::io::TripleSink) -> PyResult<()>,
) -> PyResult<Option<String>> {
    match path {
        Some(p) => {
            let mut out = RdfWriter::create(Some(p), &plan.prefixes, options).map_err(|e| PyIOError::new_err(e.to_string()))?;
            body(&mut out)?;
            Ok(None)
        }
        None => {
            let mut out = RdfWriter::new(Vec::new(), &plan.prefixes, options).map_err(value_error)?;
            body(&mut out)?;
            let mut bytes = out.into_inner().map_err(|e| PyIOError::new_err(e.to_string()))?;
            bytes.flush().ok();
            String::from_utf8(bytes).map(Some).map_err(value_error)
        }
    }
}

#[pymodule]
#[pyo3(name = "tarka")]
fn tarka_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Mapping>()?;
    m.add_function(wrap_pyfunction!(expand, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
