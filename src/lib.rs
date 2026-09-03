//! Versioned, bounded JSON Lines events for one local CLI execution.
//!
//! The sequence number is the ordering authority. `timestamp_unix_ms` is a
//! wall-clock observation and may move backwards or jump when the system
//! clock changes. The runner does not capture environment variables, and it
//! does not redact command arguments, so callers must avoid putting secrets in
//! argv.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::io::{self, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_EVENT_BYTES: usize = 64 * 1024;
pub const MAX_STREAM_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_EVENTS: usize = 4096;
pub const DEFAULT_TIMEOUT_MS: u64 = 60_000;
pub const MAX_TIMEOUT_MS: u64 = 10 * 60_000;
const READ_CHUNK_BYTES: usize = 4096;
const RECEIVE_WAIT_MS: u64 = 10;
const MAX_DRAIN_POLLS: usize = 512;

/// A protocol event. Optional fields are present only for event types that
/// use them, which keeps JSON Lines compact and makes field meaning explicit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub schema_version: u32,
    pub sequence: u64,
    pub timestamp_unix_ms: u64,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub argv: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancelled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancellation_reason: Option<String>,
}

impl Event {
    fn base(sequence: u64, event: &str) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            sequence,
            timestamp_unix_ms: now_unix_ms(),
            event: event.to_owned(),
            argv: None,
            cwd: None,
            pid: None,
            stream: None,
            text: None,
            status: None,
            exit_code: None,
            signal: None,
            cancelled: None,
            cancellation_reason: None,
        }
    }

    fn started(sequence: u64, argv: &[String], cwd: String, pid: u32) -> Self {
        let mut event = Self::base(sequence, "command.started");
        event.argv = Some(argv.to_vec());
        event.cwd = Some(cwd);
        event.pid = Some(pid);
        event
    }

    fn output(sequence: u64, stream: OutputStream, text: String) -> Self {
        let mut event = Self::base(sequence, "command.output");
        event.stream = Some(stream.as_str().to_owned());
        event.text = Some(text);
        event
    }

    fn finished(
        sequence: u64,
        status: &str,
        exit_code: Option<i32>,
        signal: Option<i32>,
        cancelled: bool,
        cancellation_reason: Option<String>,
    ) -> Self {
        let mut event = Self::base(sequence, "command.finished");
        event.status = Some(status.to_owned());
        event.exit_code = exit_code;
        event.signal = signal;
        event.cancelled = Some(cancelled);
        event.cancellation_reason = cancellation_reason;
        event
    }

    /// Serialize one event without a trailing newline and enforce the event
    /// size limit used by the validator and runner.
    pub fn to_json_line(&self) -> Result<String, EventError> {
        let line = serde_json::to_string(self)?;
        if line.len() > MAX_EVENT_BYTES {
            return Err(EventError::EventTooLarge(line.len()));
        }
        Ok(line)
    }
}

#[derive(Debug)]
pub enum EventError {
    EmptyCommand,
    InvalidArgument(String),
    EventTooLarge(usize),
    StreamTooLarge,
    Reader(String),
    Io(io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for EventError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCommand => write!(formatter, "command must contain at least one argument"),
            Self::InvalidArgument(message) => write!(formatter, "invalid argument: {message}"),
            Self::EventTooLarge(size) => write!(
                formatter,
                "event is {size} bytes, maximum is {MAX_EVENT_BYTES} bytes"
            ),
            Self::StreamTooLarge => write!(
                formatter,
                "captured stream exceeded {MAX_STREAM_BYTES} bytes"
            ),
            Self::Reader(message) => write!(formatter, "output reader failed: {message}"),
            Self::Io(error) => write!(formatter, "process or file error: {error}"),
            Self::Json(error) => write!(formatter, "JSON error: {error}"),
        }
    }
}

impl std::error::Error for EventError {}

impl From<io::Error> for EventError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for EventError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub schema_version: u32,
    pub valid: bool,
    pub event_count: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Summary {
    pub schema_version: u32,
    pub valid: bool,
    pub event_count: usize,
    pub command: Option<Vec<String>>,
    pub status: Option<String>,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub cancelled: Option<bool>,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub stdout_sha256: String,
    pub stderr_sha256: String,
    pub errors: Vec<String>,
}

/// Parse and validate one JSON Lines record.
pub fn parse_event_line(line: &str) -> Result<Event, String> {
    if line.len() > MAX_EVENT_BYTES {
        return Err(format!(
            "event is {} bytes, maximum is {} bytes",
            line.len(),
            MAX_EVENT_BYTES
        ));
    }
    let event: Event = serde_json::from_str(line).map_err(|error| error.to_string())?;
    if event.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "unsupported schema_version {}, expected {}",
            event.schema_version, SCHEMA_VERSION
        ));
    }
    if event.sequence == 0 {
        return Err("sequence must start at 1".to_owned());
    }
    match event.event.as_str() {
        "command.started" => {
            if event.argv.as_ref().is_none_or(Vec::is_empty) {
                return Err("command.started requires a non-empty argv".to_owned());
            }
            if event.cwd.as_deref().is_none_or(str::is_empty) {
                return Err("command.started requires cwd".to_owned());
            }
            if event.pid.is_none() {
                return Err("command.started requires pid".to_owned());
            }
        }
        "command.output" => {
            if !matches!(event.stream.as_deref(), Some("stdout" | "stderr")) {
                return Err("command.output stream must be stdout or stderr".to_owned());
            }
            if event.text.is_none() {
                return Err("command.output requires text".to_owned());
            }
        }
        "command.finished" => {
            let Some(status) = event.status.as_deref() else {
                return Err("command.finished requires status".to_owned());
            };
            if !matches!(
                status,
                "success" | "failure" | "signal" | "cancelled" | "output_limit"
            ) {
                return Err(format!("unknown command.finished status {status}"));
            }
            if event.cancelled.is_none() {
                return Err("command.finished requires cancelled".to_owned());
            }
            if matches!(status, "success" | "failure") && event.exit_code.is_none() {
                return Err(format!("status {status} requires exit_code"));
            }
            if status == "signal" && event.signal.is_none() {
                return Err("status signal requires signal".to_owned());
            }
            if matches!(status, "cancelled" | "output_limit") && event.cancelled != Some(true) {
                return Err(format!("status {status} requires cancelled=true"));
            }
        }
        other => return Err(format!("unknown event type {other}")),
    }
    Ok(event)
}

/// Validate a complete event stream. The report is deterministic for a given
/// input and contains at most 32 error messages.
pub fn validate_stream(input: &str) -> ValidationReport {
    let mut report = ValidationReport {
        schema_version: SCHEMA_VERSION,
        valid: false,
        event_count: 0,
        errors: Vec::new(),
    };

    if input.len() > MAX_STREAM_BYTES {
        report.errors.push(format!(
            "stream is {} bytes, maximum is {} bytes",
            input.len(),
            MAX_STREAM_BYTES
        ));
        return report;
    }

    let mut events = Vec::new();
    for (line_index, raw_line) in input.lines().enumerate() {
        if raw_line.is_empty() {
            push_error(
                &mut report.errors,
                format!("line {} is empty", line_index + 1),
            );
            continue;
        }
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        match parse_event_line(line) {
            Ok(event) => {
                if events.len() >= MAX_EVENTS {
                    push_error(
                        &mut report.errors,
                        format!("stream exceeds {} events", MAX_EVENTS),
                    );
                    break;
                }
                events.push(event);
            }
            Err(error) => push_error(
                &mut report.errors,
                format!("line {}: {error}", line_index + 1),
            ),
        }
    }

    report.event_count = events.len();
    if events.is_empty() {
        push_error(&mut report.errors, "stream has no valid events".to_owned());
    } else {
        if events[0].event != "command.started" {
            push_error(
                &mut report.errors,
                "first event must be command.started".to_owned(),
            );
        }
        if events
            .last()
            .is_none_or(|event| event.event != "command.finished")
        {
            push_error(
                &mut report.errors,
                "last event must be command.finished".to_owned(),
            );
        }
        for (index, event) in events.iter().enumerate() {
            let expected = (index + 1) as u64;
            if event.sequence != expected {
                push_error(
                    &mut report.errors,
                    format!(
                        "event at position {} has sequence {}, expected {}",
                        index + 1,
                        event.sequence,
                        expected
                    ),
                );
            }
            if event.event == "command.started" && index != 0 {
                push_error(
                    &mut report.errors,
                    "command.started may appear only first".to_owned(),
                );
            }
            if event.event == "command.finished" && index + 1 != events.len() {
                push_error(
                    &mut report.errors,
                    "command.finished may appear only last".to_owned(),
                );
            }
        }
    }
    report.valid = report.errors.is_empty();
    report
}

/// Produce a deterministic summary. Timestamps, process IDs, and working
/// directories are intentionally excluded from the result.
pub fn summarize_stream(input: &str) -> Summary {
    let validation = validate_stream(input);
    let mut summary = Summary {
        schema_version: SCHEMA_VERSION,
        valid: validation.valid,
        event_count: validation.event_count,
        command: None,
        status: None,
        exit_code: None,
        signal: None,
        cancelled: None,
        stdout_bytes: 0,
        stderr_bytes: 0,
        stdout_sha256: digest_bytes(&[]),
        stderr_sha256: digest_bytes(&[]),
        errors: validation.errors,
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    for raw_line in input.lines() {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let Ok(event) = parse_event_line(line) else {
            continue;
        };
        match event.event.as_str() {
            "command.started" => summary.command = event.argv,
            "command.output" => {
                if let (Some(stream), Some(text)) = (event.stream.as_deref(), event.text) {
                    match stream {
                        "stdout" => stdout.extend_from_slice(text.as_bytes()),
                        "stderr" => stderr.extend_from_slice(text.as_bytes()),
                        _ => {}
                    }
                }
            }
            "command.finished" => {
                summary.status = event.status;
                summary.exit_code = event.exit_code;
                summary.signal = event.signal;
                summary.cancelled = event.cancelled;
            }
            _ => {}
        }
    }
    summary.stdout_bytes = stdout.len();
    summary.stderr_bytes = stderr.len();
    summary.stdout_sha256 = digest_bytes(&stdout);
    summary.stderr_sha256 = digest_bytes(&stderr);
    summary
}

/// Run one local command and return a bounded, interleaved event sequence.
/// No environment variables are copied into events. A default 60 second
/// timeout applies when `timeout_ms` is `None`.
pub fn run_command(
    argv: &[String],
    timeout_ms: Option<u64>,
    cancel_after_ms: Option<u64>,
) -> Result<Vec<Event>, EventError> {
    validate_run_options(argv, timeout_ms, cancel_after_ms)?;
    let effective_timeout = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    let current_dir = std::env::current_dir()?.to_string_lossy().into_owned();

    let mut command = Command::new(&argv[0]);
    command.args(&argv[1..]);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let pid = child.id();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| EventError::Reader("stdout pipe was unavailable".to_owned()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| EventError::Reader("stderr pipe was unavailable".to_owned()))?;

    let mut events = Vec::with_capacity(16);
    let started_event = Event::started(1, argv, current_dir, pid);
    if let Err(error) = append_event(&mut events, started_event) {
        let _ = terminate_child(&mut child, "start_event_limit");
        return Err(error);
    }

    let (sender, receiver) = mpsc::channel();
    let mut reader_handles = Vec::with_capacity(2);
    reader_handles.push(spawn_reader(OutputStream::Stdout, stdout, sender.clone()));
    reader_handles.push(spawn_reader(OutputStream::Stderr, stderr, sender));

    let started_at = Instant::now();
    let timeout = Duration::from_millis(effective_timeout);
    let cancel_after = cancel_after_ms.map(Duration::from_millis);
    let mut child_status = None;
    let mut readers_done = 0usize;
    let mut drain_polls = 0usize;
    let mut cancelled = false;
    let mut cancellation_reason = None;
    let mut reader_error = None;
    let mut stdout_capture = StreamCapture::default();
    let mut stderr_capture = StreamCapture::default();

    loop {
        if child_status.is_none() {
            if let Some(status) = child.try_wait()? {
                child_status = Some(status);
            }
        }

        if child_status.is_none() && !cancelled {
            let reason = if cancel_after.is_some_and(|limit| started_at.elapsed() >= limit) {
                Some("requested".to_owned())
            } else if started_at.elapsed() >= timeout {
                Some("timeout".to_owned())
            } else {
                None
            };
            if let Some(reason) = reason {
                child_status = Some(terminate_child(&mut child, &reason)?);
                cancelled = true;
                cancellation_reason = Some(reason);
            }
        }

        if child_status.is_some() {
            if readers_done >= 2 || drain_polls >= MAX_DRAIN_POLLS {
                break;
            }
            drain_polls += 1;
        }

        match receiver.recv_timeout(Duration::from_millis(RECEIVE_WAIT_MS)) {
            Ok(ReaderMessage::Chunk { stream, bytes }) => {
                let capture = match stream {
                    OutputStream::Stdout => &mut stdout_capture,
                    OutputStream::Stderr => &mut stderr_capture,
                };
                let (text, exceeded) = capture.accept(&bytes);
                if !text.is_empty() {
                    if events.len() >= MAX_EVENTS - 1 {
                        if child_status.is_none() {
                            child_status = Some(terminate_child(&mut child, "event_limit")?);
                        }
                        cancelled = true;
                        cancellation_reason = Some("event_limit".to_owned());
                    } else {
                        let sequence = events.len() as u64 + 1;
                        append_event(&mut events, Event::output(sequence, stream, text))?;
                    }
                }
                if exceeded && !cancelled {
                    if child_status.is_none() {
                        child_status = Some(terminate_child(&mut child, "output_limit")?);
                    }
                    cancelled = true;
                    cancellation_reason = Some("output_limit".to_owned());
                }
            }
            Ok(ReaderMessage::Done { error }) => {
                readers_done += 1;
                if let Some(error) = error {
                    if reader_error.is_none() {
                        reader_error = Some(error);
                    }
                    if child_status.is_none() {
                        child_status = Some(terminate_child(&mut child, "reader_error")?);
                        cancelled = true;
                        cancellation_reason = Some("reader_error".to_owned());
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                readers_done = 2;
            }
        }
    }

    if child_status.is_none() {
        child_status = Some(child.wait()?);
    }
    flush_capture(&mut events, &mut stdout_capture, OutputStream::Stdout)?;
    flush_capture(&mut events, &mut stderr_capture, OutputStream::Stderr)?;

    if readers_done >= 2 {
        for handle in reader_handles.drain(..) {
            let _ = handle.join();
        }
    }
    if let Some(error) = reader_error {
        return Err(EventError::Reader(error));
    }

    let status = child_status.expect("child status is set before finish");
    let (status_name, exit_code, signal) = classify_status(status, cancelled);
    let sequence = events.len() as u64 + 1;
    append_event(
        &mut events,
        Event::finished(
            sequence,
            status_name,
            exit_code,
            signal,
            cancelled,
            cancellation_reason,
        ),
    )?;
    Ok(events)
}

fn validate_run_options(
    argv: &[String],
    timeout_ms: Option<u64>,
    cancel_after_ms: Option<u64>,
) -> Result<(), EventError> {
    if argv.is_empty() || argv[0].is_empty() {
        return Err(EventError::EmptyCommand);
    }
    let timeout = timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
    if timeout == 0 || timeout > MAX_TIMEOUT_MS {
        return Err(EventError::InvalidArgument(format!(
            "timeout_ms must be between 1 and {MAX_TIMEOUT_MS}"
        )));
    }
    if let Some(cancel_after) = cancel_after_ms {
        if cancel_after > timeout {
            return Err(EventError::InvalidArgument(
                "cancel_after_ms must not exceed timeout_ms".to_owned(),
            ));
        }
    }
    Ok(())
}

fn append_event(events: &mut Vec<Event>, event: Event) -> Result<(), EventError> {
    if events.len() >= MAX_EVENTS {
        return Err(EventError::InvalidArgument(format!(
            "event count exceeds {MAX_EVENTS}"
        )));
    }
    event.to_json_line()?;
    events.push(event);
    Ok(())
}

fn push_error(errors: &mut Vec<String>, error: String) {
    if errors.len() < 32 {
        errors.push(error);
    }
}

fn digest_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputStream {
    Stdout,
    Stderr,
}

impl OutputStream {
    fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

#[derive(Debug)]
enum ReaderMessage {
    Chunk {
        stream: OutputStream,
        bytes: Vec<u8>,
    },
    Done {
        error: Option<String>,
    },
}

fn spawn_reader<R>(
    stream: OutputStream,
    mut reader: R,
    sender: Sender<ReaderMessage>,
) -> JoinHandle<()>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut buffer = [0u8; READ_CHUNK_BYTES];
        let mut error = None;
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(size) => {
                    if sender
                        .send(ReaderMessage::Chunk {
                            stream,
                            bytes: buffer[..size].to_vec(),
                        })
                        .is_err()
                    {
                        return;
                    }
                }
                Err(read_error) => {
                    error = Some(read_error.to_string());
                    break;
                }
            }
        }
        let _ = sender.send(ReaderMessage::Done { error });
    })
}

#[derive(Debug, Default)]
struct StreamCapture {
    total: usize,
    pending_utf8: Vec<u8>,
}

impl StreamCapture {
    fn accept(&mut self, bytes: &[u8]) -> (String, bool) {
        let remaining = MAX_STREAM_BYTES.saturating_sub(self.total);
        let accepted = bytes.len().min(remaining);
        self.total += accepted;
        self.pending_utf8.extend_from_slice(&bytes[..accepted]);
        let text = decode_pending(&mut self.pending_utf8);
        (text, accepted != bytes.len())
    }

    fn flush(&mut self) -> String {
        if self.pending_utf8.is_empty() {
            return String::new();
        }
        let text = String::from_utf8_lossy(&self.pending_utf8).into_owned();
        self.pending_utf8.clear();
        text
    }
}

fn decode_pending(pending: &mut Vec<u8>) -> String {
    let mut output = String::new();
    loop {
        match std::str::from_utf8(pending) {
            Ok(text) => {
                output.push_str(text);
                pending.clear();
                break;
            }
            Err(error) => {
                let valid_up_to = error.valid_up_to();
                if valid_up_to > 0 {
                    output.push_str(
                        std::str::from_utf8(&pending[..valid_up_to])
                            .expect("valid UTF-8 prefix was reported as valid"),
                    );
                    pending.drain(..valid_up_to);
                }
                if let Some(error_len) = error.error_len() {
                    output.push('\u{fffd}');
                    pending.drain(..error_len.max(1).min(pending.len()));
                } else {
                    break;
                }
            }
        }
    }
    output
}

fn flush_capture(
    events: &mut Vec<Event>,
    capture: &mut StreamCapture,
    stream: OutputStream,
) -> Result<(), EventError> {
    let text = capture.flush();
    if !text.is_empty() && events.len() < MAX_EVENTS - 1 {
        let sequence = events.len() as u64 + 1;
        append_event(events, Event::output(sequence, stream, text))?;
    }
    Ok(())
}

fn terminate_child(child: &mut Child, _reason: &str) -> Result<ExitStatus, EventError> {
    match child.kill() {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::InvalidInput => {}
        Err(error) => return Err(EventError::Io(error)),
    }
    Ok(child.wait()?)
}

fn classify_status(
    status: ExitStatus,
    cancelled: bool,
) -> (&'static str, Option<i32>, Option<i32>) {
    let exit_code = status.code();
    #[cfg(unix)]
    let signal = std::os::unix::process::ExitStatusExt::signal(&status);
    #[cfg(not(unix))]
    let signal = None;
    if cancelled {
        ("cancelled", exit_code, signal)
    } else if status.success() {
        ("success", exit_code, signal)
    } else if signal.is_some() {
        ("signal", exit_code, signal)
    } else {
        ("failure", exit_code, signal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| (*part).to_owned()).collect()
    }

    fn rendered(events: &[Event]) -> String {
        events
            .iter()
            .map(|event| event.to_json_line().expect("event should serialize"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn success_event_stream_validates() {
        let events = run_command(&command(&["sh", "-c", "printf hello"]), Some(2_000), None)
            .expect("command should run");
        let report = validate_stream(&rendered(&events));
        assert!(report.valid, "{report:?}");
        assert_eq!(
            events.first().map(|event| event.event.as_str()),
            Some("command.started")
        );
        assert_eq!(
            events.last().map(|event| event.event.as_str()),
            Some("command.finished")
        );
    }

    #[test]
    fn failure_preserves_exit_code() {
        let events = run_command(&command(&["sh", "-c", "exit 7"]), Some(2_000), None)
            .expect("command should run");
        let finished = events.last().expect("finished event");
        assert_eq!(finished.status.as_deref(), Some("failure"));
        assert_eq!(finished.exit_code, Some(7));
    }

    #[cfg(unix)]
    #[test]
    fn signal_termination_is_reported() {
        let events = run_command(&command(&["sh", "-c", "kill -TERM $$"]), Some(2_000), None)
            .expect("command should run");
        let finished = events.last().expect("finished event");
        assert_eq!(finished.status.as_deref(), Some("signal"));
        assert!(finished.signal.is_some());
    }

    #[test]
    fn cancellation_is_reported() {
        let events = run_command(&command(&["sh", "-c", "sleep 5"]), Some(2_000), Some(30))
            .expect("command should be cancelled");
        let finished = events.last().expect("finished event");
        assert_eq!(finished.status.as_deref(), Some("cancelled"));
        assert_eq!(finished.cancelled, Some(true));
        assert_eq!(finished.cancellation_reason.as_deref(), Some("requested"));
    }

    #[test]
    fn invalid_json_is_rejected() {
        let report = validate_stream("not-json\n");
        assert!(!report.valid);
        assert_eq!(report.event_count, 0);
        assert!(!report.errors.is_empty());
    }

    #[test]
    fn oversized_event_is_rejected() {
        let line = "x".repeat(MAX_EVENT_BYTES + 1);
        assert!(parse_event_line(&line).is_err());
    }

    #[test]
    fn interleaved_output_keeps_stream_names() {
        let events = run_command(
            &command(&["sh", "-c", "printf out; printf err >&2; printf done"]),
            Some(2_000),
            None,
        )
        .expect("command should run");
        let streams = events
            .iter()
            .filter_map(|event| event.stream.as_deref())
            .collect::<Vec<_>>();
        assert!(streams.contains(&"stdout"));
        assert!(streams.contains(&"stderr"));
    }

    #[test]
    fn unicode_output_survives_capture() {
        let events = run_command(
            &command(&["sh", "-c", "printf 'héllo 世界'"]),
            Some(2_000),
            None,
        )
        .expect("command should run");
        let text = events
            .iter()
            .filter_map(|event| event.text.as_deref())
            .collect::<String>();
        assert_eq!(text, "héllo 世界");
    }

    #[test]
    fn summary_is_deterministic() {
        let fixture = concat!(
            "{\"schema_version\":1,\"sequence\":1,\"timestamp_unix_ms\":10,\"event\":\"command.started\",\"argv\":[\"printf\",\"hi\"],\"cwd\":\"/tmp\",\"pid\":1}\n",
            "{\"schema_version\":1,\"sequence\":2,\"timestamp_unix_ms\":11,\"event\":\"command.output\",\"stream\":\"stdout\",\"text\":\"hi\"}\n",
            "{\"schema_version\":1,\"sequence\":3,\"timestamp_unix_ms\":12,\"event\":\"command.finished\",\"status\":\"success\",\"exit_code\":0,\"cancelled\":false}\n"
        );
        assert_eq!(summarize_stream(fixture), summarize_stream(fixture));
        assert!(summarize_stream(fixture).valid);
    }
}
