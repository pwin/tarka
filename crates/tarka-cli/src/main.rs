//! The `tarka` command.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use oxrdf::{NamedNode, Triple};
use tarka::io::{CsvOptions, CsvSource, OutputFormat, OutputOptions, RdfWriter, Split, TripleSink};
use tarka::ottr::{CompileOptions, Library};
use tarka::{Plan, RunOptions, RunStats};

#[derive(Parser)]
#[command(name = "tarka", version, about = "Turn CSV into RDF with TARQL queries or OTTR templates")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)] // parsed once per run
enum Command {
    /// Map a CSV file to RDF with a TARQL query (-q) or an OTTR template (-l … -T …)
    Run(RunArgs),
    /// Expand OTTR instances (stOTTR files) to RDF, as Lutra does
    Expand(ExpandArgs),
    /// Write SHACL shapes for the RDF a mapping makes
    Shapes(ShapesArgs),
}

/// The mapping: a TARQL query, or OTTR templates.
#[derive(Args)]
struct MappingArgs {
    /// A TARQL query (SPARQL CONSTRUCT)
    #[arg(short, long, conflicts_with_all = ["library", "template"])]
    query: Option<PathBuf>,
    /// An OTTR template file or directory (repeat for more)
    #[arg(short, long, requires = "template", action = clap::ArgAction::Append)]
    library: Vec<PathBuf>,
    /// A root template, by IRI or prefixed name; its parameters are CSV columns.
    /// Repeat it to instantiate several templates for every row.
    #[arg(short = 'T', long, requires = "library", action = clap::ArgAction::Append)]
    template: Vec<String>,
    /// The cells of a list-typed parameter are split on SEPARATOR
    #[arg(long, num_args = 2, value_names = ["COLUMN", "SEPARATOR"], action = clap::ArgAction::Append)]
    list: Vec<String>,
}

#[derive(Args)]
struct RunArgs {
    #[command(flatten)]
    mapping: MappingArgs,
    #[command(flatten)]
    csv: CsvArgs,
    #[command(flatten)]
    output: OutputArgs,
    #[cfg(feature = "shacl")]
    #[command(flatten)]
    check: CheckArgs,
    /// Records to evaluate together
    #[arg(long, default_value_t = 512)]
    batch_size: usize,
    /// Batches to work on at once (0: one per CPU)
    #[arg(long, default_value_t = 0)]
    jobs: usize,
    /// Print what was read and made to standard error
    #[arg(long)]
    stats: bool,
}

/// CSV options, as oxi-gen names them.
#[derive(Args)]
struct CsvArgs {
    /// The input: a CSV file, or a Parquet or Arrow IPC file (default: the file the query names with FROM, else standard input)
    #[arg(short, long)]
    input: Option<PathBuf>,
    /// The field delimiter
    #[arg(short, long, default_value = ",", conflicts_with = "tab")]
    delimiter: String,
    /// Fields are tab-separated
    #[arg(short, long)]
    tab: bool,
    /// The escape character inside quoted fields
    #[arg(short = 'p', long, alias = "escape_char", default_value = "\\")]
    escape_char: String,
    /// The quote character
    #[arg(long, alias = "quote_char", default_value = "\"")]
    quote_char: String,
    /// Upper-case the column names
    #[arg(short, long)]
    normalize: bool,
    /// The file has no header row: columns are a…z, A…Z
    #[arg(short = 'H', long)]
    no_header_row: bool,
    /// Repeat each row once per part of column ORIGINAL split on DELIMITER, with the part in column SPLIT
    #[arg(long, num_args = 3, value_names = ["ORIGINAL", "SPLIT", "DELIMITER"], action = clap::ArgAction::Append)]
    split: Vec<String>,
    /// Bind empty cells as empty strings instead of leaving them unbound
    #[arg(long, alias = "bind_empty_strings")]
    bind_empty_strings: bool,
    /// Read only the first N rows
    #[arg(long, num_args = 0..=1, default_missing_value = "5")]
    test: Option<u64>,
}

#[derive(Args)]
struct OutputArgs {
    /// The output file (default: standard output)
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// The output format
    #[arg(long, value_enum)]
    format: Option<Format>,
    /// Write N-Triples (the same as --format ntriples)
    #[arg(long)]
    ntriples: bool,
    /// Put every triple in this named graph (N-Quads output)
    #[arg(long)]
    graph: Option<String>,
    /// Compress the output file with gzip
    #[arg(short, long)]
    gzip: bool,
    /// Remove duplicates within a window of N triples (default: within each row)
    #[arg(long, num_args = 0..=1, default_missing_value = "1000")]
    dedup: Option<usize>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Turtle,
    Ntriples,
    Nquads,
}

/// Validation of the output with SHACL. The output is written either way; if it does
/// not conform, tarka exits with status 3.
#[cfg(feature = "shacl")]
#[derive(Args)]
struct CheckArgs {
    /// Validate the output against the shapes made from the mapping (as `tarka shapes` writes them)
    #[arg(long)]
    validate: bool,
    /// Validate the output against the SHACL shapes in FILE (repeat for more; with --validate, as well)
    #[arg(long, value_name = "FILE", action = clap::ArgAction::Append)]
    shapes: Vec<PathBuf>,
    /// Write the SHACL validation report to FILE, in Turtle
    #[arg(long, value_name = "FILE")]
    report: Option<PathBuf>,
}

#[derive(Args)]
struct ExpandArgs {
    /// An OTTR template file or directory (repeat for more)
    #[arg(short, long, required = true, action = clap::ArgAction::Append)]
    library: Vec<PathBuf>,
    /// stOTTR files with instances
    #[arg(required = true)]
    instances: Vec<PathBuf>,
    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Args)]
struct ShapesArgs {
    #[command(flatten)]
    mapping: MappingArgs,
    /// The output file (default: standard output)
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// The namespace of the shapes' IRIs
    #[arg(long, default_value = "urn:tarka:shapes:")]
    base: String,
}

/// The output was written, but does not conform to the shapes.
#[derive(Debug)]
struct NotConforming(usize);

impl fmt::Display for NotConforming {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = if self.0 == 1 { "" } else { "s" };
        write!(f, "the output does not conform to the shapes ({} result{s})", self.0)
    }
}

impl std::error::Error for NotConforming {}

fn main() -> ExitCode {
    match Cli::parse().command.run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tarka: {e:#}");
            if e.downcast_ref::<NotConforming>().is_some() { ExitCode::from(3) } else { ExitCode::FAILURE }
        }
    }
}

impl Command {
    fn run(self) -> Result<()> {
        match self {
            Self::Run(args) => run(args),
            Self::Expand(args) => expand(args),
            Self::Shapes(args) => shapes(args),
        }
    }
}

fn run(args: RunArgs) -> Result<()> {
    let plan = mapping(&args.mapping)?;
    let input = args.csv.input.clone().or_else(|| plan.default_input.as_ref().map(PathBuf::from));
    #[cfg(feature = "polars")]
    if let Some(path) = input.as_deref().filter(|p| tarka_polars::is_frame_file(p)) {
        return run_frame(&args, &plan, path);
    }
    let reader: Box<dyn Read> = match &input {
        Some(p) if p.as_os_str() != "-" => Box::new(File::open(p).with_context(|| format!("cannot open {}", p.display()))?),
        _ => Box::new(io::stdin().lock()),
    };
    let source = CsvSource::new(io::BufReader::with_capacity(1 << 16, reader), csv_options(&args.csv)?)?;
    let columns = source.columns().to_vec();
    let options = RunOptions { batch_size: args.batch_size, jobs: args.jobs };
    write_output(&args, &plan, |sink| Ok(tarka::run(&plan, &columns, source, sink, &options)?))
}

/// A Parquet or Arrow IPC input, read with Polars.
#[cfg(feature = "polars")]
fn run_frame(args: &RunArgs, plan: &Plan, path: &Path) -> Result<()> {
    if !args.csv.split.is_empty() {
        bail!("--split applies to CSV input; in a data frame, use a list column");
    }
    let mut df = tarka_polars::read_frame(path).with_context(|| path.display().to_string())?;
    if let Some(n) = args.csv.test {
        df = df.head(Some(n as usize));
    }
    let options = RunOptions { batch_size: args.batch_size, jobs: args.jobs };
    let frame = tarka_polars::FrameOptions { bind_empty_strings: args.csv.bind_empty_strings };
    write_output(args, plan, |sink| Ok(tarka_polars::run_frame(plan, &df, &frame, sink, &options)?))
}

/// Runs `body` with the output as its sink, keeping a copy of the triples to validate
/// when asked to.
fn write_output(args: &RunArgs, plan: &Plan, body: impl FnOnce(&mut dyn TripleSink) -> Result<RunStats>) -> Result<()> {
    // the shapes are read first, so that a mistake in them is found before the run
    #[cfg(feature = "shacl")]
    let check = Check::new(&args.check, plan)?;
    #[cfg(not(feature = "shacl"))]
    let check: Option<Check> = None;
    let mut out = RdfWriter::create(args.output.output.as_deref(), &plan.prefixes, output_options(&args.output)?)?;
    let mut tee = Tee { out: &mut out, kept: check.is_some().then(HashSet::new) };
    let stats = body(&mut tee)?;
    let kept = tee.kept.take();
    if args.stats {
        eprintln!("{} records, {} solutions, {} triples made, {} written", stats.records, stats.solutions, stats.triples, out.written());
    }
    match (check, kept) {
        (Some(check), Some(kept)) => check.validate(plan, &kept),
        _ => Ok(()),
    }
}

/// Writes rows to `out`, keeping the triples too when `kept` is set.
struct Tee<'a> {
    out: &'a mut dyn TripleSink,
    kept: Option<HashSet<Triple>>,
}

impl TripleSink for Tee<'_> {
    fn row(&mut self, triples: Vec<Triple>) -> io::Result<()> {
        if let Some(kept) = &mut self.kept {
            kept.extend(triples.iter().cloned());
        }
        self.out.row(triples)
    }

    fn finish(&mut self) -> io::Result<()> {
        self.out.finish()
    }
}

/// The shapes to validate the output against.
#[cfg(feature = "shacl")]
struct Check {
    shapes: Vec<Triple>,
    report: Option<PathBuf>,
}

/// Without SHACL_Engine there is nothing to validate with.
#[cfg(not(feature = "shacl"))]
enum Check {}

#[cfg(not(feature = "shacl"))]
impl Check {
    fn validate(self, _: &Plan, _: &HashSet<Triple>) -> Result<()> {
        match self {}
    }
}

#[cfg(feature = "shacl")]
impl Check {
    fn new(args: &CheckArgs, plan: &Plan) -> Result<Option<Self>> {
        if !args.validate && args.shapes.is_empty() && args.report.is_none() {
            return Ok(None);
        }
        let mut shapes = Vec::new();
        if args.validate || args.shapes.is_empty() {
            shapes.extend(tarka_shacl::shapes(plan, &Default::default()).triples());
        }
        for path in &args.shapes {
            shapes.extend(read_rdf(path).with_context(|| format!("cannot read the shapes in {}", path.display()))?);
        }
        Ok(Some(Self { shapes, report: args.report.clone() }))
    }

    fn validate(self, plan: &Plan, data: &HashSet<Triple>) -> Result<()> {
        let v = tarka_shacl::validate(data, &self.shapes)?;
        if let Some(path) = &self.report {
            let mut prefixes = plan.prefixes.clone();
            prefixes.insert("sh", tarka_shacl::SH);
            let options = OutputOptions { window: usize::MAX, ..OutputOptions::default() };
            let mut out = RdfWriter::create(Some(path), &prefixes, options).with_context(|| path.display().to_string())?;
            out.row(v.report.iter().map(|t| t.into_owned()).collect())?;
            out.finish()?;
        }
        if v.conforms {
            return Ok(());
        }
        let mut prefixes = plan.prefixes.clone();
        prefixes.insert_if_absent("sh", tarka_shacl::SH);
        prefixes.insert_if_absent("xsd", "http://www.w3.org/2001/XMLSchema#");
        let iri = |iri: &str| prefixes.compact(iri).unwrap_or_else(|| format!("<{iri}>"));
        // a term as in N-Triples, with its IRIs shortened
        let short = |t: &str| match t.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
            Some(i) => iri(i),
            None => match t.rsplit_once("^^<") {
                Some((lexical, dt)) if t.starts_with('"') => format!("{lexical}^^{}", iri(dt.trim_end_matches('>'))),
                _ => t.to_owned(),
            },
        };
        for f in v.findings.iter().take(10) {
            let path = f.path.as_deref().map(|p| format!(" {}", short(p))).unwrap_or_default();
            let value = f.value.as_deref().map(|v| format!(": {}", short(v))).unwrap_or_default();
            eprintln!("  {}{path} {}{value}", short(&f.focus_node), short(&f.component));
        }
        if v.findings.len() > 10 {
            eprintln!(
                "  … and {} more{}",
                v.findings.len() - 10,
                if self.report.is_some() { " (see the report)" } else { " (see --report)" }
            );
        }
        Err(NotConforming(v.findings.len()).into())
    }
}

/// The triples in an RDF file, in the format its extension names (Turtle if none).
#[cfg(feature = "shacl")]
fn read_rdf(path: &Path) -> Result<Vec<Triple>> {
    use oxrdfio::{RdfFormat, RdfParser};
    let format = path.extension().and_then(|e| e.to_str()).and_then(RdfFormat::from_extension).unwrap_or(RdfFormat::Turtle);
    let file = File::open(path)?;
    let mut out = Vec::new();
    for quad in RdfParser::from_format(format).for_reader(io::BufReader::new(file)) {
        let quad = quad?;
        out.push(Triple::new(quad.subject, quad.predicate, quad.object));
    }
    Ok(out)
}

fn mapping(args: &MappingArgs) -> Result<Plan> {
    if let Some(q) = &args.query {
        let text = std::fs::read_to_string(q).with_context(|| format!("cannot read {}", q.display()))?;
        return tarka::tarql::parse_tarql(&text, &stem(q)).with_context(|| q.display().to_string());
    }
    if args.template.is_empty() {
        bail!("give a TARQL query (-q) or a library and a template (-l … -T …)");
    }
    let lib = Library::load(&args.library)?;
    let lists: HashMap<String, String> = args.list.chunks(2).map(|c| (c[0].clone(), c[1].clone())).collect();
    let roots: Vec<&str> = args.template.iter().map(String::as_str).collect();
    Ok(tarka::ottr::compile_many(&lib, &roots, &CompileOptions { given: false, lists })?)
}

fn expand(args: ExpandArgs) -> Result<()> {
    let mut lib = Library::load(&args.library)?;
    let mut instances = Vec::new();
    for path in &args.instances {
        let text = std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        let doc = tarka::ottr::parse_stottr(&text, &path.display().to_string())?;
        for (p, ns) in doc.prefixes.iter() {
            lib.prefixes.insert_if_absent(p, ns);
        }
        instances.extend(doc.instances);
    }
    let mut out = RdfWriter::create(args.output.output.as_deref(), &lib.prefixes, output_options(&args.output)?)?;
    tarka::expand_instances(&lib, &instances, &mut out)?;
    Ok(())
}

fn shapes(args: ShapesArgs) -> Result<()> {
    NamedNode::new(args.base.as_str()).with_context(|| format!("--base {}", args.base))?;
    let plan = mapping(&args.mapping)?;
    let text = tarka_shacl::shapes(&plan, &tarka_shacl::ShapeOptions { base: args.base }).to_turtle();
    match &args.output {
        Some(p) if p.as_os_str() != "-" => std::fs::write(p, text).with_context(|| format!("cannot write {}", p.display()))?,
        _ => io::stdout().lock().write_all(text.as_bytes())?,
    }
    Ok(())
}

fn csv_options(a: &CsvArgs) -> Result<CsvOptions> {
    let byte = |s: &str, what: &str| -> Result<u8> {
        match s.as_bytes() {
            [b] => Ok(*b),
            _ => bail!("the {what} must be a single ASCII character"),
        }
    };
    Ok(CsvOptions {
        delimiter: if a.tab { b'\t' } else { byte(&a.delimiter, "delimiter")? },
        quote: byte(&a.quote_char, "quote character")?,
        escape: Some(byte(&a.escape_char, "escape character")?),
        has_headers: !a.no_header_row,
        normalize_headers: a.normalize,
        bind_empty_strings: a.bind_empty_strings,
        splits: a.split.chunks(3).map(|c| Split { column: c[0].clone(), name: c[1].clone(), separator: c[2].clone() }).collect(),
        limit: a.test,
    })
}

fn output_options(a: &OutputArgs) -> Result<OutputOptions> {
    let graph = a.graph.as_ref().map(|g| NamedNode::new(g.as_str())).transpose().context("--graph")?;
    let format = match (a.format, a.ntriples, &graph) {
        (Some(Format::Turtle), _, _) => OutputFormat::Turtle,
        (Some(Format::Ntriples), _, _) | (None, true, None) => OutputFormat::NTriples,
        (Some(Format::Nquads), _, _) | (None, _, Some(_)) => OutputFormat::NQuads,
        (None, false, None) => OutputFormat::Turtle,
    };
    if format == OutputFormat::NQuads && graph.is_none() {
        bail!("N-Quads output needs --graph");
    }
    Ok(OutputOptions { format, window: a.dedup.unwrap_or(0), gzip: a.gzip, graph })
}

fn stem(p: &Path) -> String {
    p.file_stem().map_or_else(|| "query".into(), |s| s.to_string_lossy().into_owned())
}
