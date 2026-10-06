//! Runtime values: an RDF term, or a list (an OTTR list argument).

use oxrdf::Term;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Value {
    Term(Term),
    List(Vec<Value>),
}

impl Value {
    pub fn as_term(&self) -> Option<&Term> {
        match self {
            Self::Term(t) => Some(t),
            Self::List(_) => None,
        }
    }
}

impl<T: Into<Term>> From<T> for Value {
    fn from(t: T) -> Self {
        Self::Term(t.into())
    }
}
