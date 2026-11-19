//! SPARQL endpoint sources: the SELECT query sent to the endpoint with the SPARQL 1.1
//! Protocol (a POST of `application/sparql-query`), its results read as JSON or XML.

use holos_engine::{oxrdf, sparesults, spargebra};
use oxrdf::Term;
use sparesults::{QueryResultsFormat, QueryResultsParser, SliceQueryResultsParserOutput};
use spargebra::{Query, SparqlParser};

/// The rows a SELECT query gives at an endpoint: the variable names, and one value per
/// variable and solution (None when unbound).
pub fn rows(url: &str, query: &str) -> Result<crate::Table<Term>, String> {
    let parsed = SparqlParser::new().parse_query(query).map_err(|e| format!("the SPARQL query: {e}"))?;
    let Query::Select { pattern, .. } = &parsed else {
        return Err("a SPARQL endpoint source's ottr:query must be a SELECT query".into());
    };
    tarka_tarql::sparql11::check(&[], pattern).map_err(|e| e.to_string())?;
    let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).build().into();
    let mut response = agent
        .post(url)
        .header("Accept", "application/sparql-results+json, application/sparql-results+xml;q=0.9")
        .content_type("application/sparql-query")
        .send(query)
        .map_err(|e| format!("cannot query {url}: {e}"))?;
    let status = response.status().as_u16();
    let media = response.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or_default().to_owned();
    let body = response.body_mut().with_config().limit(u64::MAX).read_to_vec().map_err(|e| format!("{url}: {e}"))?;
    if !(200..300).contains(&status) {
        let text = String::from_utf8_lossy(&body);
        return Err(format!("{url} answered {status}: {}", text.trim().chars().take(500).collect::<String>()));
    }
    let media = media.split(';').next().unwrap_or_default().trim();
    let format = QueryResultsFormat::from_media_type(media)
        .filter(|f| matches!(f, QueryResultsFormat::Json | QueryResultsFormat::Xml))
        .ok_or_else(|| format!("{url} answered with {media:?}, not SPARQL results in JSON or XML"))?;
    table(url, format, &body)
}

/// The rows of SELECT results in JSON or XML. An RDF 1.2 term in them (a triple term, or a
/// literal with a base direction) is an error.
fn table(url: &str, format: QueryResultsFormat, body: &[u8]) -> Result<crate::Table<Term>, String> {
    let parsed = QueryResultsParser::from_format(format).for_slice(body).map_err(|e| format!("{url}: the results: {e}"))?;
    let SliceQueryResultsParserOutput::Solutions(solutions) = parsed else {
        return Err(format!("{url} answered a SELECT query with a boolean"));
    };
    let variables: Vec<String> = solutions.variables().iter().map(|v| v.as_str().to_owned()).collect();
    let mut out = Vec::new();
    for solution in solutions {
        let solution = solution.map_err(|e| format!("{url}: the results: {e}"))?;
        let row: Vec<Option<Term>> = variables.iter().map(|v| solution.get(v.as_str()).cloned()).collect();
        if let Some(t) = row.iter().flatten().find(|t| crate::rdf::rdf_12(t)) {
            return Err(format!("{url}: {t} is RDF 1.2, and tarka reads RDF 1.1"));
        }
        out.push(row);
    }
    Ok((variables, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A literal with a base direction is refused from XML results as from JSON: sparesults,
    /// built with HOLOS's SPARQL 1.2 features, reads `its:dir` in both.
    #[test]
    fn a_base_direction_is_refused_in_json_and_xml() {
        let json =
            br#"{"head":{"vars":["x"]},"results":{"bindings":[{"x":{"type":"literal","value":"a","xml:lang":"ar","its:dir":"rtl"}}]}}"#;
        let xml = br#"<?xml version="1.0"?><sparql xmlns="http://www.w3.org/2005/sparql-results#" xmlns:its="http://www.w3.org/2005/11/its"><head><variable name="x"/></head><results><result><binding name="x"><literal xml:lang="ar" its:dir="rtl">a</literal></binding></result></results></sparql>"#;
        for (format, body) in [(QueryResultsFormat::Json, &json[..]), (QueryResultsFormat::Xml, &xml[..])] {
            let error = table("http://e", format, body).unwrap_err();
            assert!(error.contains("is RDF 1.2"), "{format:?}: {error}");
        }
        let plain = br#"{"head":{"vars":["x"]},"results":{"bindings":[{"x":{"type":"literal","value":"a","xml:lang":"ar"}}]}}"#;
        assert_eq!(table("http://e", QueryResultsFormat::Json, plain).unwrap().1.len(), 1);
    }
}
