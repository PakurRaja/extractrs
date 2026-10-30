# Extractrs

### Clean web content. Fast Rust. Yours to keep.

[![CI](https://img.shields.io/badge/CI-placeholder-lightgrey)](https://github.com/OWNER/extractrs/actions)
[![Crates.io](https://img.shields.io/badge/crates.io-not_published-lightgrey)](https://crates.io/crates/extractrs)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue.svg)](#license)

**Extractrs is a local-first Rust CLI that turns webpages into clean Markdown, plain text, or structured JSON—ready for developers, AI agents, and RAG pipelines.**

## What it does

Give Extractrs a URL and it fetches the page, identifies its main content, removes common page clutter, and returns a clean result you can read, save, or pipe into another tool. It also extracts useful page metadata and can split content into retrieval-friendly chunks.

Extractrs is built for **AI and RAG developers, researchers, indie hackers, and anyone building tools around web content**. Extraction happens on your machine: page content is not sent to a hosted extraction API. Fetching a public URL still requires an internet connection.

## Why Extractrs?

- **Local-first and privacy-conscious:** no account, API key, or hosted extraction service.
- **Fast and lightweight:** a native Rust binary with bounded response sizes and concurrent batch requests.
- **Useful output:** readable Markdown, plain text, structured JSON, or JSON Lines.
- **Agent- and RAG-friendly:** metadata, deterministic schema-guided field extraction, and overlapping content chunks.
- **Simple to automate:** predictable CLI options, stdout/file output, and batch URL input.
- **No browser automation:** uses HTTP and HTML parsing—no headless browser to install or run.

## Quick start

```bash
# Print clean Markdown
extractrs https://example.com

# Get structured JSON with metadata
extractrs https://example.com --format json --pretty

# Save extracted content
extractrs https://quotes.toscrape.com -o quotes.md
```

## Demo

### Markdown

```bash
extractrs https://example.com
```

Representative output:

```markdown
# Example Domain

This domain is for use in illustrative examples in documents. You may use this
domain in literature without prior coordination or asking for permission.

[More information...](https://www.iana.org/domains/example)
```

### Structured JSON

```bash
extractrs https://httpbin.org/html --format json --pretty
```

Output shape (page metadata varies by source):

```json
{
  "title": "Herman Melville - Moby-Dick",
  "content": "# Herman Melville - Moby-Dick\n\nCall me Ishmael...",
  "url": "https://httpbin.org/html",
  "metadata": {
    "author": null,
    "description": null,
    "site_name": null,
    "language": null,
    "published_time": null,
    "modified_time": null,
    "canonical_url": null,
    "favicon": null,
    "open_graph": {}
  }
}
```

### Article page

```bash
extractrs https://quotes.toscrape.com --format markdown
```

Extractrs aims to retain the page's useful text and structure while leaving navigation and other common boilerplate behind. Results depend on the HTML each site serves.

## Installation

### Cargo

Once published to crates.io:

```bash
cargo install extractrs
```

### GitHub Releases

Download a prebuilt binary for your platform from the [Releases page](https://github.com/PakurRaja/extractrs/releases). Release binaries are not available until published.

### Build from source

```bash
git clone https://github.com/PakurRaja/extractrs.git
cd extractrs
cargo build --release
./target/release/extractrs --help
```

To build a statically linked Linux binary, install the `x86_64-unknown-linux-musl` Rust target and a musl linker, then run:

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

The resulting binary is at `target/x86_64-unknown-linux-musl/release/extractrs`.

## Usage examples

```bash
# Default: clean Markdown to stdout
extractrs https://example.com

# Explicit Markdown output
extractrs https://quotes.toscrape.com --format markdown

# Pretty structured JSON
extractrs https://example.com --format json --pretty

# Save to a file
extractrs https://quotes.toscrape.com -o quotes.md

# Limit extraction to a CSS selector
extractrs https://httpbin.org/html --selector "body"

# Return only page identity and metadata
extractrs https://example.com --metadata-only --pretty

# Extract a batch from a URL-per-line file
extractrs --batch urls.txt --format jsonl

# Create overlapping RAG chunks (character counts, not tokens)
extractrs https://example.com --chunk-size 1200 --overlap 150

# Extract fields available in page metadata, JSON-LD, or semantic HTML
extractrs https://example.com --schema '{"title":"string","author":"string"}'
extractrs https://example.com --schema @schema.json

# Adjust request timeout and User-Agent
extractrs https://example.com --timeout 15 --user-agent "MyResearchBot/1.0"
```

### Output formats

| Format     | Use                                                                                      |
| ---------- | ---------------------------------------------------------------------------------------- |
| `markdown` | Readable content that preserves useful headings, links, lists, and code. Default.        |
| `text`     | Plain text with paragraph breaks.                                                        |
| `json`     | A page object (or array for multiple URLs) containing title, content, URL, and metadata. |
| `jsonl`    | One compact JSON object per line, useful for streaming and batch pipelines.              |

`--schema` is deterministic best-effort extraction, not LLM-based semantic inference. It supports `string`, `number`, `integer`, `boolean`, `array`, and `object` fields. Missing or incompatible values are represented explicitly.

Batch requests run concurrently (up to eight at a time), preserve input order, and show progress on stderr. Use `--quiet` to suppress progress output.

## Extractrs vs. hosted readers

|                                        | Extractrs                                          | Firecrawl / Jina Reader                      |
| -------------------------------------- | -------------------------------------------------- | -------------------------------------------- |
| Where extraction runs                  | Locally on your machine                            | Hosted service                               |
| Page content sent to an extraction API | No                                                 | Typically processed by the service           |
| Setup                                  | Install or build a CLI binary                      | Use a hosted API or product                  |
| Best fit                               | Private, scriptable extraction and local pipelines | Hosted crawling, APIs, and managed workflows |

Extractrs is a **local-first alternative to Firecrawl and Jina Reader** for focused webpage extraction. It prioritizes fast, private CLI use and low operational overhead; it is not a hosted crawler or a drop-in replacement for their broader managed platforms. Fetching a webpage requires network access, but extraction and formatting happen locally.

## Roadmap

- Continue improving main-content extraction quality across diverse sites.
- Expand metadata and structured-data coverage.
- Grow schema-guided extraction and RAG chunking capabilities.
- Explore optional local caching and change detection.

## Contributing

Issues, bug reports, and pull requests are welcome. For changes, please run:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache License 2.0](LICENSE-APACHE), at your option.

## GitHub Pages

The static landing page is in [`docs/`](docs/). To publish it from this repository:

1. Push the `docs/index.html` file to the repository's default branch.
2. Open the repository on GitHub and go to **Settings → Pages**.
3. Under **Build and deployment**, choose **Deploy from a branch**.
4. Select the default branch and the `/docs` folder, then click **Save**.
5. Wait for the deployment shown on the Pages settings screen, then open the published URL.

Alternatively, build or copy the site into the root of a `gh-pages` branch and select that branch with the `/ (root)` folder in the same Pages settings. GitHub Pages deploys the static files; no build step is required.
