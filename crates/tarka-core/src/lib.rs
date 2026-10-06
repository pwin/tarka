//! The mapping plan shared by tarka's front ends and backends.
//!
//! A TARQL query and an OTTR template both compile to a [`Plan`]: a *lifting* layer
//! that turns a row into values, and a *shape* layer of triple patterns, each with
//! the variables it requires. Backends run plans: row by row, or column-wise.

pub mod exec;
pub mod literal;
pub mod plan;
pub mod prefix;
pub mod record;
pub mod value;

pub use exec::{Emitter, Labels};
pub use plan::{
    BNodeId, Block, CellSource, ColumnBinding, Conversion, Expander, Lifting, Pattern, Plan, Repeat, SparqlLifting, TermPat, TypeRef,
    VarId, VarInfo,
};
pub use prefix::PrefixMap;
pub use record::Record;
pub use value::Value;
