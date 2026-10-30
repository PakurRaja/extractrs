use anyhow::Result;
use serde::Serialize;

use crate::{
    cli::Format,
    convert,
    extract::{ExtractedPage, PageMetadata},
    rag::{self, Chunk},
    schema::{self, FieldSchema},
};

#[derive(Serialize)]
struct MetadataOnly<'a> {
    title: &'a str,
    url: &'a str,
    metadata: &'a PageMetadata,
}

#[derive(Serialize)]
struct ChunkedPage<'a> {
    title: &'a str,
    url: &'a str,
    metadata: &'a PageMetadata,
    chunks: Vec<Chunk>,
}

#[derive(Serialize)]
struct ChunkRecord<'a> {
    title: &'a str,
    url: &'a str,
    metadata: &'a PageMetadata,
    chunk_index: usize,
    content: &'a str,
}

pub fn render(
    pages: &[ExtractedPage],
    format: Format,
    pretty: bool,
    metadata_only: bool,
    chunk_size: Option<usize>,
    overlap: usize,
    schema: Option<&FieldSchema>,
) -> Result<String> {
    anyhow::ensure!(
        !(format == Format::Jsonl && pretty),
        "--pretty cannot be used with --format jsonl"
    );
    if metadata_only {
        let metadata: Vec<_> = pages
            .iter()
            .map(|page| MetadataOnly {
                title: &page.title,
                url: &page.url,
                metadata: &page.metadata,
            })
            .collect();
        if format == Format::Jsonl {
            let lines = metadata
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(lines.join("\n") + if lines.is_empty() { "" } else { "\n" });
        }
        return if pages.len() == 1 {
            if pretty {
                Ok(serde_json::to_string_pretty(&metadata[0])?)
            } else {
                Ok(serde_json::to_string(&metadata[0])?)
            }
        } else if pretty {
            Ok(serde_json::to_string_pretty(&metadata)?)
        } else {
            Ok(serde_json::to_string(&metadata)?)
        };
    }

    if let Some(schema) = schema {
        let records: Vec<_> = pages
            .iter()
            .map(|page| schema::extract_fields(page, schema))
            .collect();
        return render_json_values(&records, format, pretty);
    }

    if let Some(size) = chunk_size {
        let chunked: Vec<_> = pages
            .iter()
            .map(|page| {
                Ok(ChunkedPage {
                    title: &page.title,
                    url: &page.url,
                    metadata: &page.metadata,
                    chunks: rag::chunk_markdown(&page.content, size, overlap)?,
                })
            })
            .collect::<Result<_>>()?;
        if format == Format::Jsonl {
            let mut lines = Vec::new();
            for page in &chunked {
                for chunk in &page.chunks {
                    lines.push(serde_json::to_string(&ChunkRecord {
                        title: page.title,
                        url: page.url,
                        metadata: page.metadata,
                        chunk_index: chunk.index,
                        content: &chunk.content,
                    })?);
                }
            }
            return Ok(lines.join("\n") + if lines.is_empty() { "" } else { "\n" });
        }
        return if pretty {
            Ok(serde_json::to_string_pretty(&chunked)?)
        } else {
            Ok(serde_json::to_string(&chunked)?)
        };
    }

    match format {
        Format::Json if pages.len() == 1 => {
            let page = &pages[0];
            if pretty {
                Ok(serde_json::to_string_pretty(page)?)
            } else {
                Ok(serde_json::to_string(page)?)
            }
        }
        Format::Json => {
            if pretty {
                Ok(serde_json::to_string_pretty(pages)?)
            } else {
                Ok(serde_json::to_string(pages)?)
            }
        }
        Format::Jsonl => {
            let lines = pages
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(lines.join("\n") + if lines.is_empty() { "" } else { "\n" })
        }
        Format::Markdown => Ok(pages
            .iter()
            .map(convert::markdown)
            .collect::<Vec<_>>()
            .join("\n---\n\n")),
        Format::Text => Ok(pages
            .iter()
            .map(|page| page.text.trim())
            .collect::<Vec<_>>()
            .join("\n\n---\n\n")
            + "\n"),
    }
}

fn render_json_values(
    values: &[serde_json::Value],
    format: Format,
    pretty: bool,
) -> Result<String> {
    if format == Format::Jsonl {
        let lines = values
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(lines.join("\n") + if lines.is_empty() { "" } else { "\n" });
    }
    if values.len() == 1 {
        return if pretty {
            Ok(serde_json::to_string_pretty(&values[0])?)
        } else {
            Ok(serde_json::to_string(&values[0])?)
        };
    }
    if pretty {
        Ok(serde_json::to_string_pretty(values)?)
    } else {
        Ok(serde_json::to_string(values)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::{ExtractOptions, extract};

    #[test]
    fn metadata_only_outputs_structured_metadata_without_content() {
        let page = extract(
            "https://example.com/page",
            r#"
                <html lang="en">
                  <head>
                    <title>Example</title>
                    <meta name="author" content="A. Writer">
                    <link rel="canonical" href="/preferred">
                  </head>
                  <body><article><p>This is enough article content to test extraction.</p></article></body>
                </html>
            "#,
            &ExtractOptions { selector: None },
        )
        .expect("page should extract");

        let json = render(&[page], Format::Markdown, true, true, None, 0, None)
            .expect("JSON should serialize");
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("metadata output should be JSON");

        assert_eq!(value["title"], "Example");
        assert_eq!(value["metadata"]["author"], "A. Writer");
        assert_eq!(
            value["metadata"]["canonical_url"],
            "https://example.com/preferred"
        );
        assert!(value.get("content").is_none());
    }

    #[test]
    fn regular_json_keeps_a_consistent_content_shape() {
        let page = extract(
            "https://example.com/page",
            r#"<article><p>Readable content for output checks.</p></article>"#,
            &ExtractOptions { selector: None },
        )
        .expect("page should extract");

        let json = render(&[page], Format::Json, false, false, None, 0, None)
            .expect("JSON should serialize");
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("page output should be JSON");

        assert!(value["content"].is_string());
        assert!(value["url"].is_string());
        assert!(value["metadata"]["open_graph"].is_object());
        assert!(value.get("text").is_none());
    }

    #[test]
    fn chunk_output_has_metadata_indexes_and_bounded_content() {
        let page = extract(
            "https://example.com/page",
            r#"<article><p>Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron.</p></article>"#,
            &ExtractOptions { selector: None },
        )
        .expect("page should extract");

        let json = render(&[page], Format::Markdown, false, false, Some(24), 5, None)
            .expect("chunk output should serialize");
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("chunk output should be JSON");
        let chunks = value[0]["chunks"].as_array().expect("chunks array");

        assert!(chunks.len() > 1);
        assert_eq!(value[0]["url"], "https://example.com/page");
        for (index, chunk) in chunks.iter().enumerate() {
            assert_eq!(chunk["index"], index);
            assert!(chunk["content"].as_str().unwrap().chars().count() <= 24);
        }
    }

    #[test]
    fn jsonl_writes_one_page_record_per_line() {
        let pages = ["First page", "Second page"]
            .iter()
            .map(|title| {
                extract(
                    "https://example.com/page",
                    &format!(
                        "<html><head><title>{title}</title></head><body><article><p>This is readable content for the requested page.</p></article></body></html>"
                    ),
                    &ExtractOptions { selector: None },
                )
                .expect("page should extract")
            })
            .collect::<Vec<_>>();

        let jsonl = render(&pages, Format::Jsonl, false, false, None, 0, None)
            .expect("JSONL output should serialize");
        let records = jsonl.lines().collect::<Vec<_>>();

        assert_eq!(records.len(), 2);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(records[0]).unwrap()["title"],
            "First page"
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(records[1]).unwrap()["title"],
            "Second page"
        );
    }

    #[test]
    fn schema_output_is_independent_of_display_format() {
        let page = extract(
            "https://example.com/product",
            r#"<html><body><h1>Widget</h1><span itemprop="price">12.50</span></body></html>"#,
            &ExtractOptions { selector: None },
        )
        .expect("page should extract");
        let schema = schema::parse_schema(r#"{"title":"string","price":"number"}"#).unwrap();
        let json = render(
            &[page],
            Format::Markdown,
            false,
            false,
            None,
            0,
            Some(&schema),
        )
        .expect("schema output should serialize");
        let value: serde_json::Value =
            serde_json::from_str(&json).expect("schema output should be JSON");

        assert_eq!(value["title"], "Widget");
        assert_eq!(value["price"], 12.5);
        assert_eq!(value.as_object().unwrap().len(), 2);
    }

    #[test]
    fn chunk_jsonl_emits_one_record_for_each_chunk() {
        let page = extract(
            "https://example.com/page",
            r#"<article><p>Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron.</p></article>"#,
            &ExtractOptions { selector: None },
        )
        .expect("page should extract");
        let jsonl = render(&[page], Format::Jsonl, false, false, Some(20), 3, None)
            .expect("chunk JSONL should serialize");
        let records = jsonl
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();

        assert!(records.len() > 1);
        assert!(
            records
                .iter()
                .all(|record| record["url"] == "https://example.com/page")
        );
        assert_eq!(records[0]["chunk_index"], 0);
    }
}
