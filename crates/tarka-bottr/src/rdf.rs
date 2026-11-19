//! RDF file sources: files loaded into one dataset, queried with a SPARQL SELECT.

use holos_engine::{oxrdf, oxrdfio, spareval, spargebra};
use std::path::{Path, PathBuf};

use oxrdf::{Dataset, Term};
use oxrdfio::{RdfFormat, RdfParser};
use spareval::QueryResults;
use spargebra::{Query, SparqlParser};

/// The rows a SELECT query gives over the files: the variable names, and one value per
/// variable and solution (None when unbound).
pub fn rows(files: &[PathBuf], query: &str) -> Result<crate::Table<Term>, String> {
    let mut dataset = Dataset::new();
    for file in files {
        load(file, &mut dataset)?;
    }
    let parsed = SparqlParser::new().parse_query(query).map_err(|e| format!("the SPARQL query: {e}"))?;
    let Query::Select { pattern, .. } = &parsed else {
        return Err("an RDF source's ottr:query must be a SELECT query".into());
    };
    tarka_tarql::sparql11::check_local(&[], pattern).map_err(|e| e.to_string())?;
    let QueryResults::Solutions(solutions) =
        tarka_tarql::evaluator().prepare(&parsed).execute(&dataset).map_err(|e| format!("the SPARQL query: {e}"))?
    else {
        unreachable!("a SELECT query gives solutions")
    };
    let variables: Vec<String> = solutions.variables().iter().map(|v| v.as_str().to_owned()).collect();
    let mut out = Vec::new();
    for solution in solutions {
        let solution = solution.map_err(|e| format!("the SPARQL query: {e}"))?;
        out.push(variables.iter().map(|v| solution.get(v.as_str()).cloned()).collect());
    }
    Ok((variables, out))
}

/// Loads an RDF file, in the format its extension names (Turtle if none).
fn load(file: &Path, dataset: &mut Dataset) -> Result<(), String> {
    let format = file.extension().and_then(|e| e.to_str()).and_then(RdfFormat::from_extension).unwrap_or(RdfFormat::Turtle);
    let reader = std::fs::File::open(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let base = format!("file:///{}", file.display().to_string().replace('\\', "/").trim_start_matches('/'));
    let parser = RdfParser::from_format(format).with_base_iri(&base).map_err(|e| e.to_string())?;
    for quad in parser.for_reader(std::io::BufReader::new(reader)) {
        let quad = quad.map_err(|e| format!("{}: {e}", file.display()))?;
        if rdf_12(&quad.object) {
            return Err(format!("{}: {} is RDF 1.2, and tarka reads RDF 1.1", file.display(), quad.object));
        }
        dataset.insert(&quad);
    }
    Ok(())
}

/// A triple term, or a literal with a base direction.
pub(crate) fn rdf_12(t: &Term) -> bool {
    match t {
        Term::NamedNode(_) | Term::BlankNode(_) => false,
        Term::Literal(l) => l.datatype().as_str() == "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString",
        #[allow(unreachable_patterns)]
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An RDF source's query has Jena's `afn:` functions, as Lutra (on Jena) has them, not
    /// HOLOS's: no XML name ends `<http://example.com/a/123>`, so its local name is "".
    /// `afn:sprintf` is refused, as tarka does not format as Java does.
    #[test]
    fn jena_functions_in_an_rdf_source() {
        let file = std::env::temp_dir().join(format!("tarka-rdf-afn-{}.ttl", std::process::id()));
        std::fs::write(
            &file,
            "<http://example.com/a/123> <http://example.com/p> 1 .\n<http://example.com/a/b-1> <http://example.com/p> 2 .\n",
        )
        .unwrap();
        let afn = "PREFIX afn: <http://jena.apache.org/ARQ/function#>";
        let local = rows(&[file.clone()], &format!("{afn} SELECT ?s (afn:localname(?s) AS ?l) WHERE {{ ?s ?p ?o }} ORDER BY ?s"));
        let sprintf = rows(&[file.clone()], &format!("{afn} SELECT (afn:sprintf(\"%05d\", ?o) AS ?f) WHERE {{ ?s ?p ?o }}"));
        std::fs::remove_file(&file).ok();
        let locals: Vec<String> = local
            .unwrap()
            .1
            .iter()
            .map(|r| match &r[1] {
                Some(Term::Literal(l)) => l.value().to_owned(),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(locals, ["", "b-1"]);
        assert!(sprintf.unwrap_err().contains("afn:sprintf is not supported"));
    }
}
