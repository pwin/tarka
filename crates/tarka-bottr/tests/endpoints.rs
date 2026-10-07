//! SPARQL endpoint sources: the request tarka sends, and its answers read as Lutra
//! reads them. With `HOLOS_SERVER` set, the endpoint is a live HOLOS serving the data
//! of the RDF file fixture, and the output must be that fixture's (and, with
//! `LUTRA_JAR`, Lutra's against the same endpoint).

#![cfg(feature = "endpoints")]

#[path = "../../tarka/tests/common/mod.rs"]
mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use common::*;
use oxrdf::{Graph, Triple};
use tarka::ottr::Library;

/// The RDF file fixture's map, with the endpoint as its source.
fn endpoint_map(url: &str) -> String {
    std::fs::read_to_string(fixture("bottr/rdf.bottr.ttl")).unwrap().replace(
        r#"ottr:source [ a ottr:RDFFileSource ; ottr:sourceURL "data.ttl" ]"#,
        &format!(r#"ottr:source [ a ottr:SPARQLEndpointSource ; ottr:sourceURL "{url}" ]"#),
    )
}

fn run_map(text: &str) -> Result<(Graph, u64), tarka_bottr::BottrError> {
    let lib = Library::load(&[fixture("bottr/templates.stottr")]).unwrap();
    let maps = tarka_bottr::parse(text, "endpoint.bottr.ttl", &std::env::temp_dir())?;
    assert!(matches!(maps[0].source, tarka_bottr::Source::SparqlEndpoint(_)));
    let mut triples: Vec<Triple> = Vec::new();
    let stats = tarka_bottr::run(&lib, &maps, &mut triples, &mut |_| {})?;
    Ok((triples.into_iter().collect(), stats.dropped))
}

struct Request {
    line: String,
    content_type: Option<String>,
    accept: Option<String>,
    body: String,
}

/// A server for one request, answering `status` with a body of type `media`.
fn serve_once(status: &'static str, media: &'static str, body: &'static str) -> (String, std::thread::JoinHandle<Request>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/sparql", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let mut request = Request { line: line.trim_end().to_owned(), content_type: None, accept: None, body: String::new() };
        let mut length = 0;
        loop {
            let mut h = String::new();
            reader.read_line(&mut h).unwrap();
            let Some((name, value)) = h.trim_end().split_once(':') else { break };
            let value = value.trim().to_owned();
            match name.to_ascii_lowercase().as_str() {
                "content-type" => request.content_type = Some(value),
                "accept" => request.accept = Some(value),
                "content-length" => length = value.parse().unwrap(),
                _ => {}
            }
        }
        let mut received = vec![0; length];
        reader.read_exact(&mut received).unwrap();
        request.body = String::from_utf8(received).unwrap();
        write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {media}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
            .unwrap();
        request
    });
    (url, server)
}

/// The data of `bottr/data.ttl`, as an endpoint answers the fixture's query.
const RESULTS: &str = r#"{"head":{"vars":["s","name","age","k"]},"results":{"bindings":[
  {"s":{"type":"uri","value":"http://example.com/ns#a"},"name":{"type":"literal","value":"Alice"},
   "age":{"type":"literal","value":"42","datatype":"http://www.w3.org/2001/XMLSchema#integer"},"k":{"type":"bnode","value":"k1"}},
  {"s":{"type":"uri","value":"http://example.com/ns#b"},"name":{"type":"literal","value":"Bob","xml:lang":"en"},
   "k":{"type":"bnode","value":"k1"}}]}}"#;

#[test]
fn queries_go_over_the_sparql_protocol() {
    let (url, server) = serve_once("200 OK", "application/sparql-results+json; charset=utf-8", RESULTS);
    let (graph, dropped) = run_map(&endpoint_map(&url)).unwrap();
    let request = server.join().unwrap();
    assert_eq!(request.line, "POST /sparql HTTP/1.1");
    assert_eq!(request.content_type.as_deref(), Some("application/sparql-query"));
    assert!(request.accept.as_deref().unwrap().starts_with("application/sparql-results+json"));
    assert!(request.body.starts_with("PREFIX ex: <http://example.com/ns#> SELECT ?s ?name ?age ?k"), "{}", request.body);
    // the same rows as the RDF file source, so the same RDF
    assert_same(&read_graph(&fixture("bottr/expected/rdf.ttl")), &graph, "endpoint");
    assert_eq!(dropped, 0);
}

#[test]
fn an_endpoints_refusal_is_the_error() {
    let fails = |status: &'static str, media: &'static str, body: &'static str| {
        let (url, server) = serve_once(status, media, body);
        let error = run_map(&endpoint_map(&url)).unwrap_err().to_string();
        server.join().unwrap();
        error
    };
    let e = fails("400 Bad Request", "text/plain", "Parse error at line 1");
    assert!(e.contains("answered 400: Parse error at line 1"), "{e}");
    let e = fails("200 OK", "text/html", "<html>a login page</html>");
    assert!(e.contains("answered with \"text/html\", not SPARQL results"), "{e}");
    let e = run_map(&endpoint_map("http://127.0.0.1:9/sparql")).unwrap_err().to_string();
    assert!(e.contains("cannot query http://127.0.0.1:9/sparql"), "{e}");
}

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        self.0.kill().ok();
        self.0.wait().ok();
    }
}

#[test]
fn a_live_endpoint_gives_the_files_output() {
    let Ok(holos) = std::env::var("HOLOS_SERVER") else {
        eprintln!("HOLOS_SERVER is not set: not querying HOLOS");
        return;
    };
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let listen = format!("127.0.0.1:{port}");
    let _server = Server(
        Command::new(holos)
            .args(["--listen", &listen, "--data"])
            .arg(fixture("bottr/data.ttl"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let start = Instant::now();
    while TcpStream::connect(&listen).is_err() {
        assert!(start.elapsed() < Duration::from_secs(30), "HOLOS did not start");
        std::thread::sleep(Duration::from_millis(100));
    }
    let map = endpoint_map(&format!("http://{listen}/query"));
    let expected = read_graph(&fixture("bottr/expected/rdf.ttl"));
    assert_same(&expected, &run_map(&map).unwrap().0, "HOLOS as the endpoint");
    if let Ok(lutra) = std::env::var("LUTRA_JAR") {
        let dir = std::env::temp_dir().join(format!("tarka-endpoint-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (path, out) = (dir.join("endpoint.bottr.ttl"), dir.join("lutra.ttl"));
        std::fs::write(&path, &map).unwrap();
        let run = Command::new("java")
            .args(["-jar", &lutra, "-m", "expand", "-I", "bottr", "-L", "stottr", "-O", "wottr"])
            .arg("-l")
            .arg(fixture("bottr/templates.stottr"))
            .arg("-o")
            .arg(&out)
            .arg(&path)
            .output()
            .unwrap();
        assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
        assert_same(&read_graph(&out), &run_map(&map).unwrap().0, "Lutra against HOLOS");
        std::fs::remove_dir_all(&dir).ok();
    }
}
