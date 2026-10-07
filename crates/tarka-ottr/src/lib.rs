//! OTTR templates for tarka.
//!
//! A template compiles to a plan by expanding it symbolically down to `ottr:Triple`:
//!
//! * a mandatory parameter becomes a requirement of every triple the instance makes,
//!   so the whole instance disappears when its argument is `none`;
//! * a default becomes "the value, or else the default";
//! * blank nodes are fresh for every instance;
//! * `cross`, `zipMin` and `zipMax` over constant lists are unrolled, and over list
//!   values become repeats that run once per element.
//!
//! The root template's values come from, in this order: its `tq:` annotations (a
//! SPARQL WHERE clause, as written by ottr2sparql's `decompose`); or the columns named
//! after its parameters, converted by the parameters' types; or OTTR instances.

pub mod compile;
pub mod lexer;
pub mod library;
pub mod model;
pub mod parser;

use thiserror::Error;

pub use compile::{CompileOptions, compile, compile_many, instance_values, unroll_instance};
pub use library::Library;
pub use model::{Document, Instance, Kind, OTerm, Param, Template};
pub use parser::parse_stottr;

/// How deep templates may nest before tarka assumes a cycle.
pub const MAX_DEPTH: usize = 64;

#[derive(Debug, Error)]
pub enum OttrError {
    #[error("{file}:{line}: {message}")]
    Syntax { file: String, line: usize, message: String },
    #[error("{0}: {1}")]
    Io(String, std::io::Error),
    #[error("template <{0}> is not defined")]
    UnknownTemplate(String),
    #[error("<{template}> takes {expected} arguments but is given {given}")]
    Arity { template: String, expected: usize, given: usize },
    #[error("<{0}> is a signature without a pattern")]
    NoPattern(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("templates nest more than {MAX_DEPTH} deep at <{0}>: is there a cycle?")]
    TooDeep(String),
    #[error("?{var} in <{template}> is not one of its parameters")]
    UnboundVariable { template: String, var: String },
    #[error("invalid IRI <{0}>")]
    Iri(String),
    #[error("the tq: lifting of <{0}>: {1}")]
    Lifting(String, tarka_tarql::TarqlError),
}
