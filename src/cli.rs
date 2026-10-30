use std::path::PathBuf;

use clap::{ArgGroup, Parser, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Markdown,
    Json,
    Jsonl,
    Text,
}

#[derive(Debug, Parser)]
#[command(
    name = "extractrs",
    version,
    about = "Extract clean, LLM-ready content from webpages",
    long_about = "Fetch webpages over HTTP and extract their main content as Markdown, plain text, or JSON."
)]
#[command(group(ArgGroup::new("verbosity").args(["verbose", "quiet"]).multiple(false)))]
pub struct Cli {
    /// One or more webpage URLs
    #[arg(value_name = "URL")]
    pub urls: Vec<String>,

    /// Output format
    #[arg(long, value_enum, default_value = "markdown")]
    pub format: Format,

    /// Extract only the first matching CSS selector
    #[arg(short, long, value_name = "CSS")]
    pub selector: Option<String>,

    /// Write output to a file instead of stdout
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,

    /// Read URLs from a file (one URL per line; blank lines and # comments are ignored)
    #[arg(long, value_name = "FILE")]
    pub batch: Option<PathBuf>,

    /// HTTP User-Agent header
    #[arg(long, value_name = "STRING")]
    pub user_agent: Option<String>,

    /// Request timeout in seconds
    #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u64).range(1..))]
    pub timeout: u64,

    /// Pretty-print JSON output
    #[arg(long)]
    pub pretty: bool,

    /// Output only title, URL, and metadata as JSON
    #[arg(long, conflicts_with_all = ["chunk_size", "schema"])]
    pub metadata_only: bool,

    /// Split Markdown content into chunks of this many characters
    #[arg(long, value_name = "CHARS", value_parser = parse_positive_usize, conflicts_with = "schema")]
    pub chunk_size: Option<usize>,

    /// Number of characters shared between adjacent chunks
    #[arg(long, default_value_t = 0, value_name = "CHARS", value_parser = clap::value_parser!(usize), requires = "chunk_size")]
    pub overlap: usize,

    /// Extract fields described by a JSON object schema (inline JSON or @FILE)
    #[arg(long, value_name = "JSON", conflicts_with = "metadata_only")]
    pub schema: Option<String>,

    /// Increase logging verbosity (-vv for debug)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Suppress progress messages and logs
    #[arg(short, long)]
    pub quiet: bool,
}

fn parse_positive_usize(value: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|error| format!("invalid character count: {error}"))?;
    if parsed == 0 {
        return Err("value must be greater than zero".to_owned());
    }
    Ok(parsed)
}
