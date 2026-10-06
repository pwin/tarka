//! Input and output for tarka: CSV read the way oxi-gen reads it, and RDF writers.

pub mod csv_source;
pub mod write;

pub use csv_source::{CsvError, CsvOptions, CsvSource, Split};
pub use tarka_core::Record;
pub use write::{OutputFormat, OutputOptions, RdfWriter, TripleSink};
