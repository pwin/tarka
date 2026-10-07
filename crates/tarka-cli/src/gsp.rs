//! Loading into a store with the SPARQL 1.1 Graph Store Protocol: HOLOS (`/graph`),
//! Oxigraph (`/store`), Fuseki (`/dataset/data`) and others.
//!
//! The triples are streamed as N-Triples while they are made: the request runs on its own
//! thread, reading from a pipe the run writes to, so nothing is held in memory. A run that
//! fails aborts the request before its body ends, so the store never takes part of it.

use std::io::{self, BufWriter, PipeReader, PipeWriter, Read};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use anyhow::{Context, Result, bail};
use oxrdf::Triple;
use tarka::core::PrefixMap;
use tarka::io::{OutputFormat, OutputOptions, RdfWriter, TripleSink};

/// The endpoint with the graph named: `?graph=IRI`, or `?default` for the default graph,
/// unless the endpoint already names one.
pub fn graph_url(endpoint: &str, graph: Option<&str>) -> String {
    let query = endpoint.split_once('?').map_or("", |(_, q)| q);
    let names_one = query.split('&').any(|p| matches!(p.split_once('=').map_or(p, |(k, _)| k), "graph" | "default"));
    if names_one {
        return endpoint.to_owned();
    }
    let sep = if endpoint.contains('?') { '&' } else { '?' };
    match graph {
        Some(g) => format!("{endpoint}{sep}graph={}", encode(g)),
        None => format!("{endpoint}{sep}default"),
    }
}

/// Percent-encodes everything but RFC 3986's unreserved characters.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// An upload in progress: a sink whose triples go to the store.
pub struct Upload {
    url: String,
    writer: RdfWriter<BufWriter<PipeWriter>>,
    request: JoinHandle<Result<(u16, String), ureq::Error>>,
    aborted: Arc<AtomicBool>,
}

/// The request body: the pipe, which ends in an error rather than at its end when the
/// run was abandoned.
struct Body {
    pipe: PipeReader,
    aborted: Arc<AtomicBool>,
}

impl Read for Body {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.pipe.read(buf)?;
        if n == 0 && !buf.is_empty() && self.aborted.load(Ordering::SeqCst) {
            return Err(io::Error::other("the run failed"));
        }
        Ok(n)
    }
}

impl Upload {
    /// Starts a request to `url`: PUT (replace the graph) or POST (add to it), with these
    /// extra headers (`Name: value`). Duplicates are removed within `window` triples.
    pub fn start(url: String, replace: bool, headers: &[String], window: usize) -> Result<Self> {
        let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).build().into();
        let mut request = if replace { agent.put(&url) } else { agent.post(&url) };
        for h in headers {
            let Some((name, value)) = h.split_once(':') else { bail!("--header {h:?}: give it as \"Name: value\"") };
            request = request.header(name.trim(), value.trim());
        }
        let request = request.content_type("application/n-triples");
        let (pipe, writer) = io::pipe().context("cannot make a pipe for the upload")?;
        let aborted = Arc::new(AtomicBool::new(false));
        let body = Body { pipe, aborted: aborted.clone() };
        let request = std::thread::spawn(move || {
            let mut response = request.send(ureq::SendBody::from_owned_reader(body))?;
            let status = response.status().as_u16();
            let body = response.body_mut().read_to_string().unwrap_or_default();
            Ok((status, body))
        });
        let options = OutputOptions { format: OutputFormat::NTriples, window, ..OutputOptions::default() };
        let writer = RdfWriter::new(BufWriter::with_capacity(1 << 16, writer), &PrefixMap::new(), options)?;
        Ok(Self { url, writer, request, aborted })
    }

    pub fn written(&self) -> u64 {
        self.writer.written()
    }

    /// Ends the body and waits for the store's answer.
    pub fn complete(self) -> Result<String> {
        let Self { url, writer, request, .. } = self;
        let finished = writer.into_inner().and_then(|w| w.into_inner().map_err(io::IntoInnerError::into_error));
        // closing the pipe ends the request body
        drop(finished);
        let answer = request.join().map_err(|_| anyhow::anyhow!("the upload to {url} failed"))?;
        let (status, body) = answer.with_context(|| format!("cannot load into {url}"))?;
        if !(200..300).contains(&status) {
            let body = body.trim();
            let detail = if body.is_empty() { String::new() } else { format!(": {}", body.chars().take(500).collect::<String>()) };
            bail!("{url} answered {status}{detail}");
        }
        Ok(format!("{url} answered {status}"))
    }

    /// Gives up on the upload after the run failed. The store's error is the one to
    /// report when it refused the request first (the run then sees a broken pipe).
    pub fn abandon(self, run: anyhow::Error) -> anyhow::Error {
        self.aborted.store(true, Ordering::SeqCst);
        let Self { url, writer, request, .. } = self;
        drop(writer);
        let broken = run.chain().any(|e| e.downcast_ref::<io::Error>().is_some_and(|e| e.kind() == io::ErrorKind::BrokenPipe));
        match request.join() {
            Ok(Ok((status, body))) if broken && !(200..300).contains(&status) => {
                anyhow::anyhow!("{url} answered {status}: {}", body.trim().chars().take(500).collect::<String>())
            }
            Ok(Err(e)) if broken => anyhow::Error::new(e).context(format!("cannot load into {url}")),
            _ => run,
        }
    }
}

impl TripleSink for Upload {
    fn row(&mut self, triples: Vec<Triple>) -> io::Result<()> {
        self.writer.row(triples)
    }

    fn finish(&mut self) -> io::Result<()> {
        self.writer.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_urls() {
        let holos = "http://127.0.0.1:7878/graph";
        assert_eq!(graph_url(holos, Some("http://example.com/g#1")), "http://127.0.0.1:7878/graph?graph=http%3A%2F%2Fexample.com%2Fg%231");
        assert_eq!(graph_url(holos, None), "http://127.0.0.1:7878/graph?default");
        assert_eq!(graph_url("http://h/store?x=1", None), "http://h/store?x=1&default");
        // the endpoint already names the graph
        assert_eq!(graph_url("http://h/graph?graph=urn:g", Some("urn:other")), "http://h/graph?graph=urn:g");
        assert_eq!(graph_url("http://h/graph?default", Some("urn:other")), "http://h/graph?default");
    }
}
