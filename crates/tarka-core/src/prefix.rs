//! Prefix declarations: expansion of prefixed names, and compaction for output.

use std::collections::HashMap;

/// An ordered prefix → namespace map. A later declaration of the same prefix replaces
/// the earlier one, as in SPARQL and Turtle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrefixMap {
    entries: Vec<(String, String)>,
    index: HashMap<String, usize>,
}

impl PrefixMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declares `prefix` (without the colon) as `namespace`.
    pub fn insert(&mut self, prefix: impl Into<String>, namespace: impl Into<String>) {
        let (prefix, namespace) = (prefix.into(), namespace.into());
        match self.index.get(&prefix) {
            Some(&i) => self.entries[i].1 = namespace,
            None => {
                self.index.insert(prefix.clone(), self.entries.len());
                self.entries.push((prefix, namespace));
            }
        }
    }

    /// Declares `prefix` only if it is not declared yet.
    pub fn insert_if_absent(&mut self, prefix: impl Into<String>, namespace: impl Into<String>) {
        let prefix = prefix.into();
        if !self.index.contains_key(&prefix) {
            self.insert(prefix, namespace);
        }
    }

    pub fn get(&self, prefix: &str) -> Option<&str> {
        self.index.get(prefix).map(|&i| self.entries[i].1.as_str())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|(p, n)| (p.as_str(), n.as_str()))
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The IRI of a prefixed name such as `ex:alice`, or `None` if it has no colon or
    /// its prefix is not declared. Like `tarql:expandPrefixedName`, it does not check
    /// the local part; the caller validates the IRI.
    pub fn expand(&self, pname: &str) -> Option<String> {
        let (prefix, local) = pname.split_once(':')?;
        Some(format!("{}{local}", self.get(prefix)?))
    }

    /// `ns:local` for `iri`, using the longest matching namespace whose remainder is a
    /// simple local name; `None` if there is none.
    pub fn compact(&self, iri: &str) -> Option<String> {
        let mut best: Option<(&str, usize, &str)> = None; // (prefix, namespace length, local)
        for (prefix, ns) in self.iter() {
            if let Some(local) = iri.strip_prefix(ns)
                && is_simple_local(local)
                && best.is_none_or(|(_, len, _)| ns.len() > len)
            {
                best = Some((prefix, ns.len(), local));
            }
        }
        best.map(|(prefix, _, local)| format!("{prefix}:{local}"))
    }
}

impl<P: Into<String>, N: Into<String>> FromIterator<(P, N)> for PrefixMap {
    fn from_iter<T: IntoIterator<Item = (P, N)>>(iter: T) -> Self {
        let mut map = Self::new();
        for (p, n) in iter {
            map.insert(p, n);
        }
        map
    }
}

fn is_simple_local(local: &str) -> bool {
    let mut chars = local.chars();
    match chars.next() {
        None => true,
        Some(c) if c.is_alphanumeric() || c == '_' => {
            local.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.')) && !local.ends_with('.')
        }
        Some(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_and_compact() {
        let mut pm: PrefixMap = [("ex", "http://example.com/"), ("", "http://default/")].into_iter().collect();
        assert_eq!(pm.expand("ex:alice").as_deref(), Some("http://example.com/alice"));
        assert_eq!(pm.expand(":x").as_deref(), Some("http://default/x"));
        assert_eq!(pm.expand("nope:x"), None);
        assert_eq!(pm.expand("plain"), None);
        pm.insert("exs", "http://example.com/sub/");
        assert_eq!(pm.compact("http://example.com/sub/a").as_deref(), Some("exs:a"));
        assert_eq!(pm.compact("http://example.com/a b"), None);
        pm.insert("ex", "http://other/");
        assert_eq!(pm.get("ex"), Some("http://other/"));
        assert_eq!(pm.len(), 3);
    }
}
