mod cli;
mod convert;
mod error;
mod extract;
mod fetch;
mod output;
mod rag;
mod schema;

use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use clap::Parser;
use cli::Cli;
use extract::ExtractOptions;
use fetch::Fetcher;
use indicatif::{ProgressBar, ProgressStyle};
use tokio::{sync::Semaphore, task::JoinSet};
use tracing::info;

const DEFAULT_USER_AGENT: &str = concat!("extractrs/", env!("CARGO_PKG_VERSION"));

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    configure_logging(cli.verbose, cli.quiet);

    let mut urls = cli.urls.clone();
    if let Some(batch_path) = &cli.batch {
        urls.extend(read_urls(batch_path).await?);
    }
    if urls.is_empty() {
        anyhow::bail!("provide at least one URL or a file with --batch");
    }
    let batch_mode = cli.batch.is_some() || urls.len() > 1;
    anyhow::ensure!(
        !(cli.format == cli::Format::Jsonl && cli.pretty),
        "--pretty cannot be used with --format jsonl"
    );
    if let Some(size) = cli.chunk_size {
        anyhow::ensure!(
            cli.overlap < size,
            "--overlap must be smaller than --chunk-size"
        );
    }

    let field_schema = if let Some(schema) = &cli.schema {
        let schema_text = if let Some(path) = schema.strip_prefix('@') {
            tokio::fs::read_to_string(path)
                .await
                .with_context(|| format!("could not read schema file {path}"))?
        } else {
            schema.clone()
        };
        Some(schema::parse_schema(&schema_text)?)
    } else {
        None
    };

    let fetcher = Fetcher::new(
        cli.user_agent.as_deref().unwrap_or(DEFAULT_USER_AGENT),
        Duration::from_secs(cli.timeout),
    )?;
    let concurrency = Arc::new(Semaphore::new(8));
    let progress = if batch_mode && !cli.quiet {
        let progress = ProgressBar::new(urls.len() as u64);
        progress.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}",
            )?
            .progress_chars("##-"),
        );
        Some(progress)
    } else {
        None
    };
    let mut jobs = JoinSet::new();
    for (index, url) in urls.iter().cloned().enumerate() {
        let fetcher = fetcher.clone();
        let concurrency = Arc::clone(&concurrency);
        let selector = cli.selector.clone();
        jobs.spawn(async move {
            let _permit = concurrency.acquire_owned().await?;
            info!(url = %url, "fetching webpage");
            let html = fetcher
                .fetch(&url)
                .await
                .with_context(|| format!("failed to fetch {url}"))?;
            let options = ExtractOptions {
                selector: selector.as_deref(),
            };
            let page = extract::extract(&url, &html, &options)
                .with_context(|| format!("could not extract content from {url}"))?;
            Ok::<_, anyhow::Error>((index, page))
        });
    }

    let mut pages: Vec<Option<extract::ExtractedPage>> =
        std::iter::repeat_with(|| None).take(urls.len()).collect();
    while let Some(result) = jobs.join_next().await {
        let (index, page) = result.context("batch worker task failed")??;
        if let Some(progress) = &progress {
            progress.set_message(urls[index].clone());
            progress.inc(1);
        }
        pages[index] = Some(page);
    }
    if let Some(progress) = progress {
        progress.finish_with_message("complete");
    }
    let pages = pages
        .into_iter()
        .map(|page| page.context("batch worker did not produce a page"))
        .collect::<Result<Vec<_>>>()?;

    let rendered = output::render(
        &pages,
        cli.format,
        cli.pretty,
        cli.metadata_only,
        cli.chunk_size,
        cli.overlap,
        field_schema.as_ref(),
    )?;
    if let Some(path) = cli.output {
        tokio::fs::write(&path, rendered)
            .await
            .with_context(|| format!("could not write output file {}", path.display()))?;
    } else {
        print!("{rendered}");
    }
    Ok(())
}

fn configure_logging(verbose: u8, quiet: bool) {
    if quiet || verbose == 0 {
        return;
    }
    let level = if verbose > 1 { "debug" } else { "info" };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

async fn read_urls(path: &PathBuf) -> Result<Vec<String>> {
    let file = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("could not read batch file {}", path.display()))?;
    Ok(file
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect())
}
