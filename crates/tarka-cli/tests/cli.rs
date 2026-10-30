//! The `tarka` command, run as a process.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures").join(rel)
}

fn tarka(args: &[&str], stdin: Option<&[u8]>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tarka"))
        .args(args)
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = stdin {
        child.stdin.take().unwrap().write_all(input).unwrap();
    }
    child.wait_with_output().unwrap()
}

fn ok(args: &[&str], stdin: Option<&[u8]>) -> String {
    let out = tarka(args, stdin);
    assert!(out.status.success(), "tarka {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

fn path(p: &Path) -> &str {
    p.to_str().unwrap()
}

fn sorted_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text.lines().filter(|l| !l.is_empty()).map(str::to_owned).collect();
    lines.sort();
    lines
}

/// oxi-gen's own options work: `-q -i -o --ntriples`, `--escape_char`, `--quote_char`.
#[test]
fn oxigen_command_lines() {
    let dir = std::env::temp_dir().join(format!("tarka-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("out.nt");
    let q = fixture("oxigen/successor_field.rq");
    let csv = fixture("oxigen/successor_field.csv");
    ok(&["run", "-q", path(&q), "-i", path(&csv), "-o", path(&out), "--ntriples", "--escape_char", "\\", "--quote_char", "\""], None);
    let written = std::fs::read_to_string(&out).unwrap();
    assert_eq!(sorted_lines(&written).len(), 6, "{written}");
    // the same from standard input to standard output
    let piped = ok(&["run", "-q", path(&q), "--ntriples"], Some(&std::fs::read(&csv).unwrap()));
    assert_eq!(sorted_lines(&piped), sorted_lines(&written));
    // gzip
    let gz = dir.join("out.nt.gz");
    ok(&["run", "-q", path(&q), "-i", path(&csv), "-o", path(&gz), "--ntriples", "--gzip"], None);
    let mut unzipped = String::new();
    flate2_read(&gz, &mut unzipped);
    assert_eq!(sorted_lines(&unzipped), sorted_lines(&written));
    std::fs::remove_dir_all(&dir).ok();
}

fn flate2_read(path: &Path, out: &mut String) {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(&bytes[..2], &[0x1f, 0x8b], "a gzip file");
    flate2::read::GzDecoder::new(&bytes[..]).read_to_string(out).unwrap();
}

/// Options and file names come in any order: a command's words, grouped as each option with
/// its values, give the same output however the groups are arranged. A bare `--dedup` or
/// `--test` before a file name takes its default, not the file name as its number.
#[test]
fn arguments_in_any_order() {
    fn orders(groups: &[Vec<String>]) -> Vec<Vec<String>> {
        if groups.is_empty() {
            return vec![vec![]];
        }
        let mut out = Vec::new();
        for i in 0..groups.len() {
            let mut rest = groups.to_vec();
            let first = rest.remove(i);
            out.extend(orders(&rest).into_iter().map(|tail| first.iter().cloned().chain(tail).collect()));
        }
        out
    }
    let group = |words: &[&str]| words.iter().map(|w| w.to_string()).collect::<Vec<_>>();
    let people = fixture("people/people.stottr");
    let cases = [
        (
            "expand",
            vec![
                group(&["-l", path(&people)]),
                group(&[path(&fixture("people/employee.inst.stottr"))]),
                group(&["--ntriples"]),
                group(&["--dedup"]),
            ],
        ),
        (
            "bottr",
            vec![
                group(&["-l", path(&people)]),
                group(&[path(&fixture("bottr/people.bottr.ttl"))]),
                group(&["--ntriples"]),
                group(&["--dedup", "50"]),
            ],
        ),
        (
            "run",
            vec![
                group(&["-q", path(&fixture("extra/people.rq"))]),
                group(&["-i", path(&fixture("extra/people.csv"))]),
                group(&["--ntriples"]),
                group(&["--test"]),
                group(&["--dedup"]),
            ],
        ),
        (
            "run",
            vec![
                group(&["-l", path(&people)]),
                group(&["-T", "ex:Employee"]),
                group(&["--list", "skills", ";"]),
                group(&["-i", path(&fixture("people/people.csv"))]),
                group(&["--ntriples", "--test", "2"]),
            ],
        ),
    ];
    for (command, groups) in cases {
        let mut first: Option<Vec<String>> = None;
        for order in orders(&groups) {
            let args: Vec<&str> = std::iter::once(command).chain(order.iter().map(String::as_str)).collect();
            let lines = sorted_lines(&ok(&args, None));
            assert!(!lines.is_empty(), "tarka {}", args.join(" "));
            match &first {
                None => first = Some(lines),
                Some(first) => assert_eq!(&lines, first, "tarka {}", args.join(" ")),
            }
        }
    }
}

/// `--test [N]` maps only the first N rows (5 when N is not given); `--dedup [N]` removes
/// duplicate triples across rows, within a window of N distinct triples (1000 when N is not
/// given), where without it duplicates are removed within each row only.
#[test]
fn test_and_dedup_options() {
    let dir = std::env::temp_dir().join(format!("tarka-test-dedup-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let query = dir.join("rows.rq");
    // each row makes a triple of its own, and one that every row makes
    std::fs::write(
        &query,
        "PREFIX ex: <http://example.com/>
         CONSTRUCT { ?s a ex:Row . ex:dataset a ex:Dataset . } WHERE { BIND(tarql:expandPrefixedName(?id) AS ?s) }",
    )
    .unwrap();
    let csv: String = std::iter::once("id\n".to_owned()).chain((0..8).map(|i| format!("ex:r{i}\n"))).collect();
    let run = |extra: &[&str]| -> String { ok(&[&["run", "-q", path(&query), "--ntriples"][..], extra].concat(), Some(csv.as_bytes())) };
    let lines = |out: &str| out.lines().filter(|l| !l.is_empty()).count();
    let dataset = |out: &str| out.lines().filter(|l| l.starts_with("<http://example.com/dataset>")).count();

    // every row, the shared triple once per row
    let all = run(&[]);
    assert_eq!((lines(&all), dataset(&all)), (16, 8), "{all}");
    // --test: the first 5 rows, or the first N
    let five = run(&["--test"]);
    assert_eq!(lines(&five), 10, "{five}");
    assert!(five.contains("<http://example.com/r4>") && !five.contains("<http://example.com/r5>"), "{five}");
    let two = run(&["--test", "2"]);
    assert_eq!(lines(&two), 4, "{two}");
    assert!(two.contains("<http://example.com/r1>") && !two.contains("<http://example.com/r2>"), "{two}");
    // --dedup: the shared triple once, within the default window of 1000 distinct triples
    let deduped = run(&["--dedup"]);
    assert_eq!((lines(&deduped), dataset(&deduped)), (9, 1), "{deduped}");
    // a window of 3 holds two rows' worth: the shared triple once per pair of rows
    let window = run(&["--dedup", "3"]);
    assert_eq!((lines(&window), dataset(&window)), (12, 4), "{window}");
    // together: the first 5 rows, the shared triple once
    let both = run(&["--test", "--dedup"]);
    assert_eq!((lines(&both), dataset(&both)), (6, 1), "{both}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn splits_tests_and_turtle() {
    let q = fixture("oxigen/splitfuncs.rq");
    let csv = fixture("oxigen/split.csv");
    let all = ok(&["run", "-q", path(&q), "-i", path(&csv), "-H", "--ntriples"], None);
    let first = ok(&["run", "-q", path(&q), "-i", path(&csv), "-H", "--ntriples", "--test", "1"], None);
    assert!(sorted_lines(&first).len() < sorted_lines(&all).len());
    let csv = "id,tags\nex:a,x;y\n";
    let q = "PREFIX ex: <http://example.com/>\nCONSTRUCT { ?s ex:tag ?tag } WHERE { BIND(tarql:expandPrefixedName(?id) AS ?s) }";
    let dir = std::env::temp_dir().join(format!("tarka-split-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("q.rq"), q).unwrap();
    let turtle = ok(&["run", "-q", path(&dir.join("q.rq")), "--split", "tags", "tag", ";"], Some(csv.as_bytes()));
    assert!(turtle.contains("@prefix ex: <http://example.com/> ."), "{turtle}");
    assert!(turtle.contains("ex:a ex:tag \"x\" , \"y\" ."), "{turtle}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn ottr_templates_modules_and_roots() {
    let retail = fixture("retail/ottr");
    let csv = fixture("retail/orders.csv");
    let by_dir = ok(&["run", "-l", path(&retail), "-T", "rt:OrderRow", "-i", path(&csv), "--ntriples"], None);
    let mut args = vec!["run".to_owned()];
    for m in ["base", "domain", "rows"] {
        args.extend(["-l".into(), retail.join(format!("{m}.stottr")).display().to_string()]);
    }
    args.extend(["-T".into(), "rt:OrderRow".into(), "-i".into(), csv.display().to_string(), "--ntriples".into()]);
    let by_module = ok(&args.iter().map(String::as_str).collect::<Vec<_>>(), None);
    assert_eq!(sorted_lines(&by_dir).len(), sorted_lines(&by_module).len());
    // two roots per row, and a list column
    let people = fixture("people/people.stottr");
    let pcsv = fixture("people/people.csv");
    let two = ok(
        &["run", "-l", path(&people), "-T", "ex:Person", "-T", "ex:Employee", "--list", "skills", ";", "-i", path(&pcsv), "--ntriples"],
        None,
    );
    assert!(two.contains("<http://example.com/ns#skill> \"rust\""), "{two}");
}

#[test]
fn expand_and_nquads() {
    let out = ok(
        &[
            "expand",
            "-l",
            path(&fixture("people/people.stottr")),
            path(&fixture("people/employee.inst.stottr")),
            "--graph",
            "http://example.com/g",
        ],
        None,
    );
    assert!(out.lines().all(|l| l.is_empty() || l.ends_with("<http://example.com/g> .")), "{out}");
    assert_eq!(sorted_lines(&out).len(), 26);
}

/// A run that fails partway leaves no part of a file behind, and an earlier file as it
/// was: the output is written under another name and moved into place once complete.
#[test]
fn a_failed_run_leaves_no_file() {
    use std::io::Read;
    let dir = std::env::temp_dir().join(format!("tarka-cli-atomic-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let lib = dir.join("lib.stottr");
    std::fs::write(
        &lib,
        "@prefix ex: <http://example.com/> . @prefix ottr: <http://ns.ottr.xyz/0.4/> .
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
         ex:Named [ ottr:IRI ?x, ?label ] :: { ottr:Triple(?x, rdfs:label, ?label) } .",
    )
    .unwrap();
    // enough instances to be written to disk before the last one fails (it lacks an argument)
    let good: String = (0..2000).map(|i| format!("ex:Named(ex:n{i}, \"name number {i}\") .\n")).collect();
    let prefixes = "@prefix ex: <http://example.com/> .\n";
    let (fine, broken) = (dir.join("fine.stottr"), dir.join("broken.stottr"));
    std::fs::write(&fine, format!("{prefixes}{good}")).unwrap();
    std::fs::write(&broken, format!("{prefixes}{good}ex:Named(ex:bad) .\n")).unwrap();
    let output = dir.join("out.ttl");
    let expand = |instances: &Path, extra: &[&str]| {
        tarka(&[&["expand", "-l", path(&lib), path(instances), "-o", path(&output)][..], extra].concat(), None)
    };
    let leftovers = || -> Vec<String> {
        let mut names: Vec<String> =
            std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.retain(|n| n.contains(".tmp"));
        names
    };

    let failed = expand(&broken, &[]);
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("argument"), "{}", String::from_utf8_lossy(&failed.stderr));
    assert!(!output.exists(), "a failed expansion leaves no file");
    assert_eq!(leftovers(), Vec::<String>::new());

    std::fs::write(&output, "previous\n").unwrap();
    assert!(!expand(&broken, &[]).status.success());
    assert_eq!(std::fs::read_to_string(&output).unwrap(), "previous\n", "an earlier file is kept as it was");

    let done = expand(&fine, &["--ntriples"]);
    assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));
    assert_eq!(std::fs::read_to_string(&output).unwrap().lines().count(), 2000);
    // a compressed file is complete once moved into place
    assert!(expand(&fine, &["--ntriples", "--gzip"]).status.success());
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(&output).unwrap()).read_to_string(&mut text).unwrap();
    assert_eq!(text.lines().count(), 2000);
    assert_eq!(leftovers(), Vec::<String>::new());

    // run alike: a ragged row near the end of the input
    let mut csv: String = "id,pref_label\n".into();
    csv.extend((0..2000).map(|i| format!("ex:n{i},name {i}\n")));
    csv.push_str("ex:ragged\n");
    let run_output = dir.join("run.nt");
    let run =
        tarka(&["run", "-q", path(&fixture("oxigen/successor_field.rq")), "--ntriples", "-o", path(&run_output)], Some(csv.as_bytes()));
    assert!(!run.status.success());
    assert!(!run_output.exists(), "a failed run leaves no file");
    assert_eq!(leftovers(), Vec::<String>::new());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn errors_are_reported_cleanly() {
    let bad_query = tarka(&["run", "-q", path(&fixture("retail/customers.csv"))], Some(b""));
    assert!(!bad_query.status.success());
    let stderr = String::from_utf8_lossy(&bad_query.stderr);
    assert!(stderr.starts_with("tarka: "), "{stderr}");
    let ragged = tarka(&["run", "-q", path(&fixture("oxigen/successor_field.rq"))], Some(b"id,pref_label\nex:a\n"));
    assert!(!ragged.status.success());
    assert!(String::from_utf8_lossy(&ragged.stderr).contains("row 0"), "{}", String::from_utf8_lossy(&ragged.stderr));
    let unknown = tarka(&["run", "-l", path(&fixture("people/people.stottr")), "-T", "ex:Nope"], Some(b"x\n"));
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("is not defined"));
    // a list parameter's text cells need a separator
    let list = tarka(
        &["run", "-l", path(&fixture("people/people.stottr")), "-T", "ex:Employee"],
        Some(b"person,name,skills\nex:a,A,rust;sparql\n"),
    );
    assert!(!list.status.success());
    assert!(String::from_utf8_lossy(&list.stderr).contains("--list skills"), "{}", String::from_utf8_lossy(&list.stderr));
}

#[test]
fn shapes_and_validation() {
    let people = fixture("extra/people.rq");
    let shapes = ok(&["shapes", "-q", path(&people)], None);
    assert!(shapes.contains("shape:PersonShape a sh:NodeShape ;\n    sh:targetClass foaf:Person ;"), "{shapes}");
    let based = ok(&["shapes", "-q", path(&people), "--base", "http://example.com/shapes/"], None);
    assert!(based.contains("@prefix shape: <http://example.com/shapes/> ."), "{based}");
    // the fixture's output conforms to the shapes made from its mapping
    ok(&["run", "-q", path(&people), "-i", path(&fixture("extra/people.csv")), "--validate"], None);

    let dir = std::env::temp_dir().join(format!("tarka-cli-shapes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let lib = dir.join("items.stottr");
    std::fs::write(
        &lib,
        "@prefix ex: <http://example.com/> . @prefix ottr: <http://ns.ottr.xyz/0.4/> . @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
         @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
         ex:Item [ ottr:IRI ?id, ? xsd:integer ?qty ] :: { ottr:Triple(?id, rdf:type, ex:Item), ottr:Triple(?id, ex:qty, ?qty) } .",
    )
    .unwrap();
    let extra = dir.join("extra.ttl");
    std::fs::write(
        &extra,
        "@prefix sh: <http://www.w3.org/ns/shacl#> . @prefix ex: <http://example.com/> .
         ex:ItemQty a sh:NodeShape ; sh:targetClass ex:Item ; sh:property [ sh:path ex:qty ; sh:minCount 1 ] .",
    )
    .unwrap();
    let (output, report) = (dir.join("items.nt"), dir.join("report.ttl"));
    let csv = b"id,qty\nex:a,7\nex:b,seven\nex:c,\n";
    let base = ["run", "-l", path(&lib), "-T", "ex:Item", "--ntriples", "-o", path(&output)];
    // the generated shapes catch the ill-typed quantity; the extra ones the missing one
    let generated = tarka(&[&base[..], &["--validate"]].concat(), Some(csv));
    assert_eq!(generated.status.code(), Some(3), "{}", String::from_utf8_lossy(&generated.stderr));
    let stderr = String::from_utf8_lossy(&generated.stderr);
    assert!(stderr.contains("ex:b ex:qty sh:DatatypeConstraintComponent: \"seven\"^^xsd:integer"), "{stderr}");
    assert!(stderr.contains("does not conform to the shapes (1 result)"), "{stderr}");
    assert_eq!(std::fs::read_to_string(&output).unwrap().lines().count(), 5, "the output is written all the same");
    let both = tarka(&[&base[..], &["--validate", "--shapes", path(&extra), "--report", path(&report)]].concat(), Some(csv));
    assert_eq!(both.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&both.stderr).contains("(2 results)"));
    let report_text = std::fs::read_to_string(&report).unwrap();
    assert!(report_text.contains("sh:conforms false"), "{report_text}");
    assert!(report_text.contains("sh:MinCountConstraintComponent"), "{report_text}");
    // --shapes alone uses only those shapes
    let only = tarka(&[&base[..], &["--shapes", path(&extra)]].concat(), Some(csv));
    assert!(String::from_utf8_lossy(&only.stderr).contains("ex:c ex:qty sh:MinCountConstraintComponent"));
    assert!(String::from_utf8_lossy(&only.stderr).contains("(1 result)"));

    // a shapes file that declares no shapes (empty, or the data by mistake) would check
    // nothing and pass, so it is refused before the run, even beside the generated shapes
    let (empty, data) = (dir.join("empty.ttl"), dir.join("data.ttl"));
    std::fs::write(&empty, "").unwrap();
    std::fs::write(&data, "@prefix ex: <http://example.com/> . ex:a a ex:Item ; ex:qty 7 .").unwrap();
    std::fs::remove_file(&output).unwrap();
    for (shapes, validate) in [(&empty, false), (&data, false), (&data, true)] {
        let args = [&base[..], &["--shapes", path(shapes)], if validate { &["--validate"][..] } else { &[] }].concat();
        let refused = tarka(&args, Some(csv));
        assert_eq!(refused.status.code(), Some(1), "{shapes:?}");
        let stderr = String::from_utf8_lossy(&refused.stderr);
        assert!(stderr.contains(&format!("{} declares no shapes", shapes.display())), "{stderr}");
        assert!(stderr.contains("--allow-no-shapes"), "{stderr}");
        assert!(!output.exists(), "nothing is written");
    }
    // unless that is what is meant
    ok(&[&base[..], &["--shapes", path(&data), "--allow-no-shapes"]].concat(), Some(csv));
    std::fs::remove_dir_all(&dir).ok();
}

/// What a one-request HTTP server was sent.
struct Received {
    request_line: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Received {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

/// An HTTP server for one request at `/graph`, answering `status` with `body`.
fn serve_once(status: &'static str, body: &'static str) -> (String, std::thread::JoinHandle<Received>) {
    use std::io::BufRead;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/graph", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let request_line = line.trim_end().to_owned();
        let mut headers = Vec::new();
        loop {
            line.clear();
            reader.read_line(&mut line).unwrap();
            match line.trim_end().split_once(':') {
                Some((name, value)) => headers.push((name.trim().to_owned(), value.trim().to_owned())),
                None => break,
            }
        }
        let mut received = Received { request_line, headers, body: String::new() };
        let mut raw = Vec::new();
        if received.header("transfer-encoding").is_some_and(|t| t.eq_ignore_ascii_case("chunked")) {
            loop {
                line.clear();
                reader.read_line(&mut line).unwrap();
                let size = usize::from_str_radix(line.trim_end().split(';').next().unwrap(), 16).unwrap();
                let mut chunk = vec![0; size + 2];
                reader.read_exact(&mut chunk).unwrap();
                if size == 0 {
                    break;
                }
                raw.extend_from_slice(&chunk[..size]);
            }
        } else if let Some(n) = received.header("content-length").map(|n| n.parse::<usize>().unwrap()) {
            raw.resize(n, 0);
            reader.read_exact(&mut raw).unwrap();
        }
        received.body = String::from_utf8(raw).unwrap();
        write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        received
    });
    (url, server)
}

#[test]
fn loading_into_a_store() {
    let (query, csv) = (fixture("extra/people.rq"), fixture("extra/people.csv"));
    let run = ["run", "-q", path(&query), "-i", path(&csv)];
    let (url, server) = serve_once("201 Created", "");
    let header = "X-Holos-Principal: urn:example:loader";
    ok(&[&run[..], &["--post", &url, "--graph", "http://example.com/g#1", "--header", header]].concat(), None);
    let received = server.join().unwrap();
    assert_eq!(received.request_line, "POST /graph?graph=http%3A%2F%2Fexample.com%2Fg%231 HTTP/1.1");
    assert_eq!(received.header("content-type"), Some("application/n-triples"));
    assert_eq!(received.header("x-holos-principal"), Some("urn:example:loader"));
    // the body is the N-Triples the run writes (blank node labels are stable)
    assert_eq!(sorted_lines(&received.body), sorted_lines(&ok(&[&run[..], &["--ntriples"]].concat(), None)));

    // --replace PUTs, and without --graph the default graph is the target
    let (url, server) = serve_once("204 No Content", "");
    let library = fixture("people/people.stottr");
    ok(&["expand", "-l", path(&library), path(&fixture("people/person.inst.stottr")), "--post", &url, "--replace"], None);
    let received = server.join().unwrap();
    assert_eq!(received.request_line, "PUT /graph?default HTTP/1.1");
    assert!(received.body.lines().count() > 0);

    // the store's refusal is the error
    let (url, server) = serve_once("403 Forbidden", "this store is read-only");
    let refused = tarka(&[&run[..], &["--post", &url]].concat(), None);
    server.join().unwrap();
    assert!(!refused.status.success());
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("?default answered 403: this store is read-only"), "{stderr}");

    // the output goes to the store or to a file, not both
    let both = tarka(&[&run[..], &["--post", "http://127.0.0.1:9/graph", "-o", "out.ttl"]].concat(), None);
    assert!(String::from_utf8_lossy(&both.stderr).contains("cannot be used with"), "{}", String::from_utf8_lossy(&both.stderr));
}

#[test]
fn lint_libraries() {
    let clean = tarka(&["lint", "-l", path(&fixture("retail/ottr"))], None);
    assert!(clean.status.success());
    assert!(String::from_utf8_lossy(&clean.stderr).contains("16 templates: 0 errors, 0 warnings"));
    let flawed = tarka(&["lint", "-l", path(&fixture("lint/flawed.stottr"))], None);
    assert!(!flawed.status.success());
    let stdout = String::from_utf8_lossy(&flawed.stdout);
    assert!(stdout.contains("flawed.stottr:20: error: ex:Nowhere is not defined [undefined-template]"), "{stdout}");
    // a parameter that may be blank as ottr:Triple's predicate: Lutra's error too
    assert!(
        stdout.contains(
            "flawed.stottr:80: error: ?p may be blank, but is given to the non-blank parameter ?predicate of ottr:Triple: \
             mark ?p non-blank (!) [non-blank-flags]"
        ),
        "{stdout}"
    );
    assert!(String::from_utf8_lossy(&flawed.stderr).contains("27 errors, 5 warnings"));
    let errors = tarka(&["lint", "-l", path(&fixture("lint/flawed.stottr")), "--errors-only"], None);
    assert!(!String::from_utf8_lossy(&errors.stdout).contains("warning:"));
    // warnings fail the run only when asked
    let dir = std::env::temp_dir().join(format!("tarka-cli-lint-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let lib = dir.join("warn.stottr");
    std::fs::write(&lib, "@prefix ex: <http://example.com/> . @prefix ottr: <http://ns.ottr.xyz/0.4/> .\nex:T [ ?a, ?unused ] :: { ottr:Triple(ex:s, ex:p, ?a) } .")
        .unwrap();
    let warned = tarka(&["lint", "-l", path(&lib)], None);
    assert!(warned.status.success());
    assert!(String::from_utf8_lossy(&warned.stdout).contains("parameter ?unused is not used"));
    assert!(!tarka(&["lint", "-l", path(&lib), "--deny-warnings"], None).status.success());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn bottr_maps() {
    let people = fixture("people/people.stottr");
    let out = ok(&["bottr", "-l", path(&people), path(&fixture("bottr/people.bottr.ttl")), "--ntriples"], None);
    assert!(out.contains("<http://example.com/ns#alice> <http://example.com/ns#skill> \"rust\" ."), "{out}");
    // instances that cannot be made are reported, the others written, and the run fails
    let templates = fixture("bottr/templates.stottr");
    let args = tarka(&["bottr", "-l", path(&templates), path(&fixture("bottr/args.bottr.ttl")), "--ntriples"], None);
    assert_eq!(args.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&args.stderr);
    assert!(stderr.contains("left out: ") && stderr.contains("row 3: argument 5 (flag)"), "{stderr}");
    assert_eq!(stderr.lines().last(), Some("tarka: 1 of 3 instances left out"), "{stderr}");
    assert_eq!(String::from_utf8_lossy(&args.stdout).lines().count(), 15);
}

/// `run -l` reads an IRI cell as bOTTR does (a declared prefix is expanded, and anything
/// else is a full IRI, so `mailto:` needs no prefix), and a row with a cell its
/// parameter cannot take is left out and reported, and the run fails, as in bOTTR.
#[test]
fn ottr_rows_that_cannot_be_made() {
    let dir = std::env::temp_dir().join(format!("tarka-left-out-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let write = |name: &str, text: &str| {
        let p = dir.join(name);
        std::fs::write(&p, text).unwrap();
        p
    };
    let template = |mbox: &str| {
        format!(
            "@prefix ex: <http://example.com/> . @prefix foaf: <http://xmlns.com/foaf/0.1/> . @prefix ottr: <http://ns.ottr.xyz/0.4/> .
             ex:Person [ ottr:IRI ?person, {mbox}ottr:IRI ?mbox, ?name ] :: {{
               ottr:Triple(?person, foaf:name, ?name), ottr:Triple(?person, foaf:mbox, ?mbox) }} ."
        )
    };
    let mandatory = write("mandatory.stottr", &template(""));
    let optional = write("optional.stottr", &template("? "));
    let good = write("good.csv", "person,mbox,name\nex:ada,mailto:ada@example.com,Ada\nhttp://example.org/bo,mailto:bo@example.com,Bo\n");
    let run = |library: &Path, csv: &Path| tarka(&["run", "-l", path(library), "-T", "ex:Person", "-i", path(csv), "--ntriples"], None);

    let by_columns = run(&mandatory, &good);
    assert!(by_columns.status.success(), "{}", String::from_utf8_lossy(&by_columns.stderr));
    let by_columns = String::from_utf8(by_columns.stdout).unwrap();
    assert!(by_columns.contains("<http://example.com/ada> <http://xmlns.com/foaf/0.1/mbox> <mailto:ada@example.com> ."), "{by_columns}");
    let map = write(
        "people.bottr.ttl",
        "@prefix ottr: <http://ns.ottr.xyz/0.4/> . @prefix ex: <http://example.com/> .
         [] a ottr:InstanceMap ; ottr:template ex:Person ; ottr:source [ a ottr:H2Source ] ;
            ottr:query \"SELECT person, mbox, name FROM CSVREAD('@@THIS_DIR@@/good.csv')\" ;
            ottr:argumentMaps ( [ ottr:type ottr:IRI ] [ ottr:type ottr:IRI ] [ ] ) .",
    );
    let by_bottr = ok(&["bottr", "-l", path(&mandatory), path(&map), "--ntriples"], None);
    assert_eq!(sorted_lines(&by_columns), sorted_lines(&by_bottr));
    assert_eq!(sorted_lines(&by_columns).len(), 4);

    // row 2's mailbox is not an IRI; row 3 has none, which is OTTR's none and not reported
    let bad = write("bad.csv", "person,mbox,name\nex:ada,mailto:ada@example.com,Ada\nex:bo,n/a,Bo\nex:cy,,Cy\n");
    for (library, made) in [(&mandatory, 2), (&optional, 3)] {
        let out = run(library, &bad);
        assert_eq!(out.status.code(), Some(1));
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("tarka: left out: row 2: column mbox: 'n/a' is not an IRI"), "{stderr}");
        assert_eq!(stderr.lines().last(), Some("tarka: 1 of 3 rows left out"), "{stderr}");
        assert_eq!(stderr.lines().count(), 2, "{stderr}");
        // the others are written; nothing of row 2, even where ?mbox is optional
        let stdout = String::from_utf8(out.stdout).unwrap();
        assert_eq!(sorted_lines(&stdout).len(), made, "{stdout}");
        assert!(!stdout.contains("/bo>"), "{stdout}");
    }
    let only_bad = write("only-bad.csv", "person,mbox,name\nex:bo,n/a,Bo\n");
    let out = run(&mandatory, &only_bad);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(stderr.lines().last(), Some("tarka: 1 of 1 row left out; nothing was made"), "{stderr}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn parquet_input() {
    use tarka_polars::polars::prelude::*;
    let mut df = df!(
        "person" => ["ex:alice", "ex:bob", "ex:carol"],
        "name" => ["Alice", "Bob", "Carol"],
        "email" => [Some("alice@example.com"), None, Some("carol@example.com")],
        "manager" => [Some("ex:carol"), Some("ex:carol"), None],
    )
    .unwrap();
    let dir = std::env::temp_dir().join(format!("tarka-parquet-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("people.parquet");
    ParquetWriter::new(std::fs::File::create(&file).unwrap()).finish(&mut df).unwrap();
    let out = ok(&["run", "-l", path(&fixture("people/people.stottr")), "-T", "ex:Person", "-i", path(&file), "--ntriples"], None);
    assert_eq!(sorted_lines(&out).len(), 16, "{out}");
    assert!(out.contains("<http://example.com/ns#alice> <http://example.com/ns#reportsTo> <http://example.com/ns#carol>"));
    let split =
        tarka(&["run", "-l", path(&fixture("people/people.stottr")), "-T", "ex:Person", "-i", path(&file), "--split", "a", "b", ";"], None);
    assert!(String::from_utf8_lossy(&split.stderr).contains("list column"));
    std::fs::remove_dir_all(&dir).ok();
}
