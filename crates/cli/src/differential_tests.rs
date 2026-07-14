//! Differential tests for the hand-rolled tools: run them against the
//! reference implementation everyone already trusts (coreutils, jq,
//! openssl) on deterministic pseudo-random inputs. Lives in the CLI
//! crate because packs are pure — they may not spawn processes even in
//! tests, but the CLI may.
#![cfg(test)]

use crate::registry;
use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};
use toolkit_core::{run_tool, DataValue, Inputs, Options};

/// Deterministic LCG so failures reproduce exactly.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[(self.next() as usize) % items.len()]
    }
}

fn run(tool: &str, input: &str, sets: &[(&str, serde_json::Value)]) -> DataValue {
    let registry = registry();
    let tool = registry.find(tool).expect("tool exists");
    let mut options = Options::new();
    for (k, v) in sets {
        options.insert((*k).into(), v.clone());
    }
    let mut inputs = Inputs::new();
    inputs.insert("input".into(), vec![DataValue::Text(input.into())]);
    run_tool(tool, inputs, &options).expect("tool runs")
}

fn pipe(cmd: &mut Command, stdin: &str) -> String {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("reference tool spawns");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("reference tool runs");
    String::from_utf8(out.stdout).expect("reference output is UTF-8")
}

fn random_lines(rng: &mut Rng, n: usize) -> String {
    let words = ["alpha", "beta", "gamma", "ab", "abba", "cab", "zz", ""];
    let mut text = String::new();
    for _ in 0..n {
        text.push_str(words[(rng.next() as usize) % words.len()]);
        text.push('\n');
    }
    text
}

#[test]
fn text_uniq_matches_coreutils() {
    for seed in 1..=5u64 {
        let input = random_lines(&mut Rng(seed), 200);
        let DataValue::Text(ours) = run("text-uniq", &input, &[]) else {
            unreachable!()
        };
        let mut our_counts = HashMap::new();
        for row in ours.lines() {
            let (count, line) = row.split_once('\t').expect("count<TAB>line");
            our_counts.insert(line.to_string(), count.parse::<u64>().expect("count"));
        }

        let sorted = pipe(&mut Command::new("sort"), &input);
        let mut uniq = Command::new("uniq");
        uniq.arg("-c");
        let reference = pipe(&mut uniq, &sorted);
        let mut ref_counts = HashMap::new();
        for row in reference.lines() {
            let row = row.trim_start();
            let (count, line) = row.split_once(' ').expect("count line");
            ref_counts.insert(line.to_string(), count.parse::<u64>().expect("count"));
        }
        assert_eq!(our_counts, ref_counts, "seed {seed}");
    }
}

#[test]
fn text_grep_matches_gnu_grep() {
    for seed in 1..=5u64 {
        let input = random_lines(&mut Rng(seed), 200);
        for invert in [false, true] {
            let DataValue::Text(ours) = run(
                "text-grep",
                &input,
                &[("pattern", "ab".into()), ("invert", invert.into())],
            ) else {
                unreachable!()
            };
            let mut cmd = Command::new("grep");
            cmd.arg("-n");
            if invert {
                cmd.arg("-v");
            }
            cmd.arg("--").arg("ab");
            let reference = pipe(&mut cmd, &input);
            assert_eq!(ours, reference, "seed {seed} invert {invert}");
        }
    }
}

#[test]
fn json_query_matches_jq() {
    // (our query, jq equivalent) over docs generated to avoid the
    // null/false edge where jq's `// empty` and JSONPath semantics
    // legitimately differ.
    let cases = [
        ("$.users[*].name", "[.users[].name]"),
        ("$.arr[-1]", "[.arr[-1]]"),
        ("$..name", "[.. | .name? // empty]"),
    ];
    for seed in 1..=5u64 {
        let mut rng = Rng(seed);
        let names = ["ada", "alan", "grace", "edsger"];
        let users: Vec<serde_json::Value> = (0..rng.next() % 5 + 1)
            .map(|_| {
                serde_json::json!({
                    "name": rng.pick(&names),
                    "age": rng.next() % 90,
                    "team": {"name": rng.pick(&names)},
                })
            })
            .collect();
        let arr: Vec<u64> = (0..rng.next() % 6 + 1).map(|_| rng.next() % 100).collect();
        let doc = serde_json::json!({"users": users, "arr": arr});
        let doc_text = doc.to_string();

        for (ours_q, jq_q) in cases {
            let DataValue::Json(ours) = run("json-query", &doc_text, &[("query", ours_q.into())])
            else {
                unreachable!()
            };
            let mut jq = Command::new("jq");
            jq.arg("-c").arg(jq_q);
            let reference = pipe(&mut jq, &doc_text);
            let reference: serde_json::Value =
                serde_json::from_str(reference.trim()).expect("jq emits JSON");
            assert_eq!(ours, reference, "seed {seed} query {ours_q}");
        }
    }
}

/// hexdump vs `xxd` on random byte lengths, including the ragged final
/// row and empty input.
#[test]
fn hexdump_matches_xxd() {
    for seed in 1..=6u64 {
        let mut rng = Rng(seed);
        // Lengths that straddle the 16-byte row boundary.
        let len = (rng.next() as usize) % 40;
        let bytes: Vec<u8> = (0..len).map(|_| (rng.next() & 0xff) as u8).collect();

        let registry = registry();
        let tool = registry.find("hexdump").expect("tool exists");
        let mut inputs = Inputs::new();
        inputs.insert("input".into(), vec![DataValue::Bytes(bytes.clone())]);
        let DataValue::Text(ours) = run_tool(tool, inputs, &Options::new()).expect("dumps") else {
            unreachable!()
        };

        let mut xxd = Command::new("xxd");
        let reference = {
            use std::io::Read;
            let mut child = xxd
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .expect("xxd spawns");
            child
                .stdin
                .take()
                .expect("stdin")
                .write_all(&bytes)
                .expect("write bytes");
            let mut out = String::new();
            child
                .stdout
                .take()
                .expect("stdout")
                .read_to_string(&mut out)
                .expect("read");
            child.wait().expect("xxd runs");
            out
        };
        assert_eq!(ours, reference, "seed {seed} len {len}");
    }
}

/// cert-decode vs a certificate openssl just minted — including an IP
/// SAN, which the fixed unit-test fixture doesn't cover.
#[test]
fn cert_decode_matches_openssl_output() {
    let dir = std::env::temp_dir().join(format!("tk-cert-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let cert_path = dir.join("cert.pem");
    let status = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "ec",
            "-pkeyopt",
            "ec_paramgen_curve:P-256",
            "-keyout",
            "/dev/null",
            "-nodes",
            "-days",
            "42",
            "-subj",
            "/CN=diff.test/O=Differential",
            "-addext",
            "subjectAltName=DNS:diff.test,IP:10.1.2.3",
            "-out",
        ])
        .arg(&cert_path)
        .stderr(Stdio::null())
        .status()
        .expect("openssl runs");
    assert!(status.success());
    let pem = std::fs::read_to_string(&cert_path).expect("cert written");
    std::fs::remove_dir_all(&dir).ok();

    let registry = registry();
    let tool = registry.find("cert-decode").expect("tool exists");
    let mut inputs = Inputs::new();
    inputs.insert("input".into(), vec![DataValue::Bytes(pem.into_bytes())]);
    let DataValue::Json(v) = run_tool(tool, inputs, &Options::new()).expect("decodes") else {
        unreachable!()
    };
    assert_eq!(v["subject"], "O=Differential,CN=diff.test");
    assert_eq!(v["self_signed"], true);
    assert_eq!(
        v["subject_alternative_names"],
        serde_json::json!(["DNS:diff.test", "IP:10.1.2.3"])
    );
}
