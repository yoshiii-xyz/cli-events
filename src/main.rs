use clap::{Parser, Subcommand};
use cli_events::{run_command, summarize_stream, validate_stream};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(
    name = "cli-events",
    version,
    about = "Record and inspect bounded CLI execution events"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run one command and emit JSON Lines events to stdout.
    Run {
        #[arg(long)]
        timeout_ms: Option<u64>,
        #[arg(long)]
        cancel_after_ms: Option<u64>,
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Validate a JSON Lines event stream.
    Validate { stream: PathBuf },
    /// Print a deterministic summary of a JSON Lines event stream.
    Summarize { stream: PathBuf },
}

fn main() -> ExitCode {
    match execute(Cli::parse()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("cli-events: {error}");
            ExitCode::from(2)
        }
    }
}

fn execute(cli: Cli) -> Result<u8, Box<dyn std::error::Error>> {
    match cli.command {
        Commands::Run {
            timeout_ms,
            cancel_after_ms,
            command,
        } => {
            let events = run_command(&command, timeout_ms, cancel_after_ms)?;
            for event in events {
                println!("{}", event.to_json_line()?);
            }
            Ok(0)
        }
        Commands::Validate { stream } => {
            let input = std::fs::read_to_string(stream)?;
            let report = validate_stream(&input);
            println!("{}", serde_json::to_string(&report)?);
            Ok(if report.valid { 0 } else { 1 })
        }
        Commands::Summarize { stream } => {
            let input = std::fs::read_to_string(stream)?;
            let summary = summarize_stream(&input);
            println!("{}", serde_json::to_string(&summary)?);
            Ok(if summary.valid { 0 } else { 1 })
        }
    }
}
