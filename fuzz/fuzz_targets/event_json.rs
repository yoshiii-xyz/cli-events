#![no_main]

use cli_events::{parse_event_line, summarize_stream, validate_stream};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);
    let _ = parse_event_line(&input);
    let _ = validate_stream(&input);
    let _ = summarize_stream(&input);
});
