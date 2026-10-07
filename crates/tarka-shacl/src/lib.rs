//! SHACL shapes for the RDF a tarka mapping makes, and (with the `validate` feature)
//! validation with [SHACL_Engine](https://github.com/pwin/SHACL_Engine).
//!
//! ```
//! let plan = tarka_tarql::parse_tarql(
//!     "PREFIX ex: <http://example.com/>
//!      CONSTRUCT { ?p a ex:Person ; ex:name ?name } WHERE { BIND(IRI(CONCAT(str(ex:), ?id)) AS ?p) }",
//!     "people",
//! )?;
//! let shapes = tarka_shacl::shapes(&plan, &Default::default());
//! assert!(shapes.to_turtle().contains("sh:targetClass ex:Person"));
//! # Ok::<_, tarka_tarql::TarqlError>(())
//! ```

pub mod kinds;
mod shapes;
#[cfg(feature = "validate")]
mod validate;

pub use shapes::{NodeKind, NodeShape, PropertyShape, SH, ShapeOptions, Shapes, Target, shapes};
#[cfg(feature = "validate")]
pub use validate::{Finding, Validation, ValidationError, validate};
