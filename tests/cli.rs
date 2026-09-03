use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cli-events"))
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/event-stream.jsonl")
}

fn run(args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .output()
        .expect("CLI should start")
}

fn temp_file(contents: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("cli-events-{}-{nonce}.jsonl", std::process::id()));
    fs::write(&path, contents).expect("temporary stream should be written");
    path
}

#[test]
fn run_emits_json_lines() {
    let output = run(&["run", "--", "sh", "-c", "printf ok"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let lines = String::from_utf8_lossy(&output.stdout);
    assert_eq!(lines.lines().count(), 3);
    for line in lines.lines() {
        let value: Value = serde_json::from_str(line).expect("each output line is JSON");
        assert_eq!(value["schema_version"], 1);
    }
}

#[test]
fn validate_accepts_fixture() {
    let output = Command::new(binary())
        .args([
            "validate",
            fixture().to_str().expect("fixture path is UTF-8"),
        ])
        .output()
        .expect("CLI should start");
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).expect("report is JSON");
    assert_eq!(report["valid"], true);
    assert_eq!(report["event_count"], 3);
}

#[test]
fn validate_rejects_invalid_json() {
    let path = temp_file("not-json\n");
    let output = Command::new(binary())
        .args(["validate", path.to_str().expect("temporary path is UTF-8")])
        .output()
        .expect("CLI should start");
    fs::remove_file(path).expect("temporary file should be removed");
    assert_eq!(output.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&output.stdout).expect("report is JSON");
    assert_eq!(report["valid"], false);
}

#[test]
fn summarize_is_stable() {
    let first = run(&[
        "summarize",
        fixture().to_str().expect("fixture path is UTF-8"),
    ]);
    let second = run(&[
        "summarize",
        fixture().to_str().expect("fixture path is UTF-8"),
    ]);
    assert!(first.status.success());
    assert_eq!(first.stdout, second.stdout);
    let summary: Value = serde_json::from_slice(&first.stdout).expect("summary is JSON");
    assert_eq!(summary["stdout_bytes"], 5);
    assert_eq!(summary["stderr_bytes"], 0);
}

#[test]
fn run_can_cancel() {
    let output = run(&[
        "run",
        "--timeout-ms",
        "2000",
        "--cancel-after-ms",
        "20",
        "--",
        "sh",
        "-c",
        "sleep 5",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rendered = String::from_utf8_lossy(&output.stdout);
    let last = rendered.lines().last().expect("finished event");
    let value: Value = serde_json::from_str(last).expect("finished event is JSON");
    assert_eq!(value["status"], "cancelled");
    assert_eq!(value["cancelled"], true);
}
