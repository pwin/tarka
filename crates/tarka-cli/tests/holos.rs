//! Loading into a running HOLOS server, live: set `HOLOS_SERVER` to the path of
//! `holos-server` (built from github.com/pwin/triplestore; `--no-default-features` gives
//! an in-memory build with no RocksDB). Without it this test passes without checking
//! anything.
//!
//! Each mapping's output is loaded into its own graph, read back over the Graph Store
//! Protocol, and must be the reference output. A run that fails must load nothing.

#[path = "../../tarka/tests/common/mod.rs"]
mod common;

use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use common::*;
use oxrdf::{Graph, Triple};
use oxrdfio::{RdfFormat, RdfParser};

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        self.0.kill().ok();
        self.0.wait().ok();
    }
}

fn tarka(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_tarka")).args(args).stdin(Stdio::null()).output().unwrap()
}

/// The graph `name` as the store has it.
fn stored(base: &str, name: &str) -> (u16, Graph) {
    let url = format!("{base}/graph?graph={}", name.replace(':', "%3A").replace('/', "%2F").replace('#', "%23"));
    let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).build().into();
    let mut response = agent.get(&url).header("Accept", "application/n-triples").call().unwrap();
    let status = response.status().as_u16();
    let text = response.body_mut().read_to_string().unwrap();
    if status != 200 {
        return (status, Graph::new());
    }
    let graph = RdfParser::from_format(RdfFormat::NTriples)
        .for_slice(text.as_bytes())
        .map(|q| {
            let q = q.unwrap();
            Triple::new(q.subject, q.predicate, q.object)
        })
        .collect();
    (status, graph)
}

#[test]
fn loads_into_holos() {
    let Ok(holos) = std::env::var("HOLOS_SERVER") else {
        eprintln!("HOLOS_SERVER is not set: not loading into HOLOS");
        return;
    };
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let listen = format!("127.0.0.1:{port}");
    let _server = Server(Command::new(holos).args(["--listen", &listen]).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap());
    let start = Instant::now();
    while TcpStream::connect(&listen).is_err() {
        assert!(start.elapsed() < Duration::from_secs(30), "HOLOS did not start");
        std::thread::sleep(Duration::from_millis(100));
    }
    let base = format!("http://{listen}");
    let endpoint = format!("{base}/graph");
    for (dataset, root) in [("customers", "rt:CustomerRow"), ("products", "rt:ProductRow"), ("orders", "rt:OrderRow")] {
        let csv = fixture(&format!("retail/{dataset}.csv"));
        let query = fixture(&format!("retail/tarql/{dataset}.rq"));
        let library = fixture("retail/ottr");
        let expected = read_graph(&fixture(&format!("retail/expected/{dataset}.nt")));
        let tarql_graph = format!("urn:tarka:test:{dataset}:tarql");
        let ottr_graph = format!("urn:tarka:test:{dataset}:ottr");
        for (args, graph) in [
            (vec!["run", "-q", query.to_str().unwrap()], &tarql_graph),
            (vec!["run", "-l", library.to_str().unwrap(), "-T", root], &ottr_graph),
        ] {
            let out = tarka(&[&args[..], &["-i", csv.to_str().unwrap(), "--post", &endpoint, "--graph", graph]].concat());
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
            let (status, actual) = stored(&base, graph);
            assert_eq!(status, 200);
            assert_same(&expected, &actual, graph);
        }
    }
    // a run that fails after streaming part of its output loads nothing
    let mut csv = String::from("id,pref_label\n");
    for i in 0..3000 {
        csv.push_str(&format!("ex:a{i},label {i}\n"));
    }
    csv.push_str("ex:ragged\n");
    let path = std::env::temp_dir().join(format!("tarka-holos-{}.csv", std::process::id()));
    std::fs::write(&path, csv).unwrap();
    let query = fixture("oxigen/successor_field.rq");
    let partial = "urn:tarka:test:partial";
    let args = ["run", "-q", query.to_str().unwrap(), "-i", path.to_str().unwrap(), "--post", &endpoint, "--graph", partial];
    let out = tarka(&[&args[..], &["--batch-size", "100", "--jobs", "1"]].concat());
    std::fs::remove_file(&path).ok();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("row 3000"), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(stored(&base, partial).0, 404, "nothing was loaded");
}
