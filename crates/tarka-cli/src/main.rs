//! The `tarka` command.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use oxrdf::NamedNode;
use tarka::io::{CsvOptions, CsvSource, OutputFormat, OutputOptions, RdfWriter, Split};
use tarka::ottr::{CompileOptions, Library};
use tarka::{Plan, RunOptions};

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
}

#[derive(Args)]
struct RunArgs {
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
    #[command(flatten)]
    csv: CsvArgs,
    #[command(flatten)]
    output: OutputArgs,
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
    /// The CSV file (default: the file the query names with FROM, else standard input)
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

fn main() -> ExitCode {
    match Cli::parse().command.run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tarka: {e:#}");
            ExitCode::FAILURE
        }
    }
}

impl Command {
    fn run(self) -> Result<()> {
        match self {
            Self::Run(args) => run(args),
            Self::Expand(args) => expand(args),
        }
    }
}

fn run(args: RunArgs) -> Result<()> {
    let plan = mapping(&args)?;
    let input = args.csv.input.clone().or_else(|| plan.default_input.as_ref().map(PathBuf::from));
    let reader: Box<dyn Read> = match &input {
        Some(p) if p.as_os_str() != "-" => Box::new(File::open(p).with_context(|| format!("cannot open {}", p.display()))?),
        _ => Box::new(io::stdin().lock()),
    };
    let source = CsvSource::new(io::BufReader::with_capacity(1 << 16, reader), csv_options(&args.csv)?)?;
    let columns = source.columns().to_vec();
    let mut out = RdfWriter::create(args.output.output.as_deref(), &plan.prefixes, output_options(&args.output)?)?;
    let options = RunOptions { batch_size: args.batch_size, jobs: args.jobs };
    let stats = tarka::run(&plan, &columns, source, &mut out, &options)?;
    if args.stats {
        eprintln!("{} records, {} solutions, {} triples made, {} written", stats.records, stats.solutions, stats.triples, out.written());
    }
    Ok(())
}

fn mapping(args: &RunArgs) -> Result<Plan> {
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
