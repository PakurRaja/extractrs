use std::collections::BTreeMap;

use dom_smoothie::{Config, Readability};
use scraper::{ElementRef, Html, Node, Selector};
use serde::Serialize;
use url::Url;

use crate::{convert, error::ExtractError};

#[derive(Debug, Clone, Serialize)]
pub struct PageMetadata {
    pub author: Option<String>,
    pub description: Option<String>,
    pub site_name: Option<String>,
    pub language: Option<String>,
    pub published_time: Option<String>,
    pub modified_time: Option<String>,
    pub canonical_url: Option<String>,
    pub favicon: Option<String>,
    pub open_graph: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExtractedPage {
    pub title: String,
    pub content: String,
    pub url: String,
    pub metadata: PageMetadata,
    #[serde(skip)]
    pub text: String,
    #[serde(skip)]
    pub structured_data: Vec<serde_json::Value>,
    #[serde(skip)]
    pub html_fields: BTreeMap<String, String>,
}

pub struct ExtractOptions<'a> {
    pub selector: Option<&'a str>,
}

pub fn extract(
    url: &str,
    html: &str,
    options: &ExtractOptions<'_>,
) -> Result<ExtractedPage, ExtractError> {
    let document = Html::parse_document(html);
    let title = first_meta(
        &document,
        &[("property", "og:title"), ("name", "twitter:title")],
    )
    .or_else(|| json_ld_string(&document, "headline"))
    .or_else(|| json_ld_string(&document, "name"))
    .or_else(|| select_text(&document, "h1"))
    .or_else(|| select_text(&document, "title"))
    .unwrap_or_default();
    let open_graph = open_graph_metadata(&document);
    let json_ld = json_ld_metadata(&document);
    let metadata = PageMetadata {
        author: first_meta(
            &document,
            &[("name", "author"), ("property", "article:author")],
        )
        .or(json_ld.author),
        description: first_meta(
            &document,
            &[("name", "description"), ("property", "og:description")],
        )
        .or(json_ld.description),
        site_name: first_meta(&document, &[("property", "og:site_name")]).or(json_ld.site_name),
        language: select_attribute(&document, "html", "lang").or(json_ld.language),
        published_time: first_meta(
            &document,
            &[
                ("property", "article:published_time"),
                ("name", "datePublished"),
            ],
        )
        .or(json_ld.published_time),
        modified_time: first_meta(
            &document,
            &[
                ("property", "article:modified_time"),
                ("property", "og:updated_time"),
                ("name", "dateModified"),
            ],
        )
        .or(json_ld.modified_time),
        canonical_url: select_attribute(&document, "link[rel~=\"canonical\"]", "href")
            .and_then(|href| resolve_url(url, &href)),
        favicon: select_attribute(
            &document,
            "link[rel~=\"icon\"], link[rel~=\"apple-touch-icon\"]",
            "href",
        )
        .and_then(|href| resolve_url(url, &href)),
        open_graph,
    };

    let (content_html, text, title) = if let Some(selector) = options.selector {
        let parsed_selector =
            Selector::parse(selector).map_err(|error| ExtractError::InvalidSelector {
                selector: selector.to_owned(),
                message: error.to_string(),
            })?;
        let element = document
            .select(&parsed_selector)
            .next()
            .ok_or_else(|| ExtractError::SelectorNotFound(selector.to_owned()))?;
        let content_html = render_children(element);
        let text = render_text(element);
        (content_html, text, title)
    } else {
        let config = Config {
            char_threshold: 250,
            max_elements_to_parse: 100_000,
            ..Config::default()
        };
        let readability_result =
            Readability::new(html, Some(url), Some(config)).and_then(|mut parser| parser.parse());
        let (readability_content, readability_error) = match readability_result {
            Ok(article) => {
                let article_title = if article.title.trim().is_empty() {
                    title.clone()
                } else {
                    article.title
                };
                (Some((article.content.to_string(), article_title)), None)
            }
            Err(error) => (None, Some(error)),
        };

        let readability_html = readability_content
            .as_ref()
            .map(|(content, _)| clean_fragment(content));
        let readability_score = readability_html
            .as_deref()
            .map(|content| content_score(content, "", 0))
            .unwrap_or_default();
        let fallback = best_semantic_container(&document);

        if let Some((content_html, text, _score)) =
            fallback.filter(|(_, _, score)| *score > readability_score && *score > 0)
        {
            (content_html, text, title)
        } else if let (Some((_, article_title)), Some(content_html)) =
            (readability_content, readability_html)
        {
            let fragment = Html::parse_fragment(&content_html);
            let text = fragment_text(&fragment);
            (content_html, text, article_title)
        } else {
            let (content_html, text, _) = best_semantic_container(&document).ok_or_else(|| {
                ExtractError::Readability(
                    readability_error.unwrap_or(dom_smoothie::ReadabilityError::GrabFailed),
                )
            })?;
            (content_html, text, title)
        }
    };

    let content = convert::html_to_markdown(&content_html);
    Ok(ExtractedPage {
        title,
        content,
        url: url.to_owned(),
        metadata,
        text,
        structured_data: json_ld_values(&document),
        html_fields: html_fields(&document),
    })
}

const SEMANTIC_SELECTORS: &[&str] = &[
    "article",
    "main",
    "[role=\"main\"]",
    "#content",
    "#main",
    ".article-content",
    ".article-body",
    ".post-content",
    ".post-body",
    ".entry-content",
    ".story-body",
    "[itemprop=\"articleBody\"]",
    "[role=\"article\"]",
    "div[id*=\"content\" i]",
    "div[id*=\"article\" i]",
    "div[class*=\"content\" i]",
    "div[class*=\"post\" i]",
    "div[class*=\"story\" i]",
    "div[class*=\"body\" i]",
    "section[class*=\"article\" i]",
];

fn best_semantic_container(document: &Html) -> Option<(String, String, i64)> {
    let mut best = None;
    for (priority, selector) in SEMANTIC_SELECTORS.iter().enumerate() {
        let Ok(selector) = Selector::parse(selector) else {
            continue;
        };
        for element in document.select(&selector) {
            if is_boilerplate(element) {
                continue;
            }
            let content_html = render_children(element);
            let text = render_text(element);
            if text.trim().is_empty() {
                continue;
            }
            let hints = format!(
                "{} {}",
                element.attr("id").unwrap_or_default(),
                element.attr("class").unwrap_or_default()
            );
            let semantic_bonus =
                match element.value().name() {
                    "article" => 160,
                    "main" => 140,
                    _ => 0,
                } + i64::try_from(SEMANTIC_SELECTORS.len().saturating_sub(priority))
                    .unwrap_or_default()
                    * 3;
            let score = content_score(&content_html, &hints, semantic_bonus);
            if best
                .as_ref()
                .is_none_or(|(_, _, best_score)| score > *best_score)
            {
                best = Some((content_html, text, score));
            }
        }
    }
    best
}

fn content_score(html: &str, hints: &str, semantic_bonus: i64) -> i64 {
    let document = Html::parse_fragment(html);
    let text = fragment_text(&document);
    let text_length = i64::try_from(text.chars().count()).unwrap_or(i64::MAX);
    let count = |selector: &str| {
        Selector::parse(selector)
            .ok()
            .map(|selector| i64::try_from(document.select(&selector).count()).unwrap_or(i64::MAX))
            .unwrap_or_default()
    };
    let paragraphs = count("p");
    let headings = count("h1, h2, h3, h4, h5, h6");
    let list_items = count("li");
    let anchor_text_length = Selector::parse("a")
        .ok()
        .map(|selector| {
            document
                .select(&selector)
                .map(|anchor| anchor.text().collect::<String>().chars().count() as i64)
                .sum::<i64>()
        })
        .unwrap_or_default();

    let link_density_penalty = if text_length > 0 {
        anchor_text_length.saturating_mul(900) / text_length
    } else {
        0
    };
    let normalized_hints = hints.to_ascii_lowercase();
    let positive_signal = [
        "article", "body", "content", "entry", "main", "post", "story",
    ]
    .iter()
    .filter(|signal| normalized_hints.contains(**signal))
    .count() as i64
        * 90;
    let has_negative_signal = [
        "advert",
        "banner",
        "breadcrumb",
        "comment",
        "cookie",
        "footer",
        "menu",
        "modal",
        "newsletter",
        "pagination",
        "popup",
        "promo",
        "related",
        "share",
        "sidebar",
        "social",
        "subscribe",
    ]
    .iter()
    .any(|signal| normalized_hints.contains(signal))
        || normalized_hints
            .split(|character: char| !character.is_ascii_alphanumeric())
            .any(|token| matches!(token, "ad" | "ads" | "adslot"));
    let negative_signal = if has_negative_signal { 700 } else { 0 };

    text_length
        .saturating_add(paragraphs.saturating_mul(110))
        .saturating_add(headings.saturating_mul(45))
        .saturating_add(list_items.saturating_mul(20))
        .saturating_add(positive_signal)
        .saturating_add(semantic_bonus)
        .saturating_sub(link_density_penalty)
        .saturating_sub(negative_signal)
}

#[derive(Default)]
struct JsonLdMetadata {
    author: Option<String>,
    description: Option<String>,
    site_name: Option<String>,
    language: Option<String>,
    published_time: Option<String>,
    modified_time: Option<String>,
}

fn open_graph_metadata(document: &Html) -> BTreeMap<String, String> {
    let Ok(selector) = Selector::parse("meta[property^=\"og:\"]") else {
        return BTreeMap::new();
    };
    let mut metadata = BTreeMap::new();
    for element in document.select(&selector) {
        let Some(property) = element.value().attr("property") else {
            continue;
        };
        let Some(content) = element.value().attr("content") else {
            continue;
        };
        let property = property.trim();
        let content = content.trim();
        if !property.is_empty() && !content.is_empty() {
            metadata
                .entry(property.to_owned())
                .or_insert_with(|| content.to_owned());
        }
    }
    metadata
}

fn json_ld_values(document: &Html) -> Vec<serde_json::Value> {
    let Ok(selector) = Selector::parse("script[type=\"application/ld+json\"]") else {
        return Vec::new();
    };
    let mut values = Vec::new();
    for script in document.select(&selector) {
        let json = script.text().collect::<String>();
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) {
            flatten_json_ld(value, &mut values);
        }
    }
    values
}

fn html_fields(document: &Html) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    let Ok(selector) = Selector::parse("[itemprop], meta[name], meta[property]") else {
        return fields;
    };
    for element in document.select(&selector) {
        let value = element
            .value()
            .attr("content")
            .map(str::to_owned)
            .unwrap_or_else(|| element.text().collect::<String>().trim().to_owned());
        if value.is_empty() {
            continue;
        }
        for attribute in ["itemprop", "name", "property"] {
            if let Some(key) = element.value().attr(attribute) {
                fields
                    .entry(key.trim().to_owned())
                    .or_insert_with(|| value.clone());
            }
        }
    }
    fields
}

fn flatten_json_ld(value: serde_json::Value, output: &mut Vec<serde_json::Value>) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                flatten_json_ld(value, output);
            }
        }
        serde_json::Value::Object(mut object) => {
            if let Some(graph) = object.remove("@graph") {
                flatten_json_ld(graph, output);
            }
            output.push(serde_json::Value::Object(object));
        }
        _ => {}
    }
}

fn json_ld_string(document: &Html, field: &str) -> Option<String> {
    json_ld_values(document)
        .iter()
        .find_map(|value| value.get(field).and_then(json_ld_value_string))
}

fn json_ld_metadata(document: &Html) -> JsonLdMetadata {
    let values = json_ld_values(document);
    JsonLdMetadata {
        author: values
            .iter()
            .find_map(|value| value.get("author").and_then(json_ld_value_string)),
        description: values
            .iter()
            .find_map(|value| value.get("description").and_then(json_ld_value_string)),
        site_name: values
            .iter()
            .find_map(|value| value.get("publisher").and_then(json_ld_value_string)),
        language: values
            .iter()
            .find_map(|value| value.get("inLanguage").and_then(json_ld_value_string)),
        published_time: values
            .iter()
            .find_map(|value| value.get("datePublished").and_then(json_ld_value_string)),
        modified_time: values
            .iter()
            .find_map(|value| value.get("dateModified").and_then(json_ld_value_string)),
    }
}

fn json_ld_value_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) => nonempty(value),
        serde_json::Value::Object(object) => object.get("name").and_then(json_ld_value_string),
        serde_json::Value::Array(values) => values.iter().find_map(json_ld_value_string),
        _ => None,
    }
}

fn nonempty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn resolve_url(base: &str, value: &str) -> Option<String> {
    let base = Url::parse(base).ok()?;
    base.join(value.trim()).ok().map(|url| url.to_string())
}

fn clean_fragment(html: &str) -> String {
    let fragment = Html::parse_fragment(html);
    render_children(fragment.root_element())
}

fn render_children(element: ElementRef<'_>) -> String {
    let mut output = String::new();
    for child in element.children() {
        match child.value() {
            Node::Text(text) => push_escaped(&mut output, &text.text, false),
            Node::Element(_) => {
                if let Some(child) = ElementRef::wrap(child) {
                    render_element(child, &mut output);
                }
            }
            _ => {}
        }
    }
    output
}

fn render_element(element: ElementRef<'_>, output: &mut String) {
    let name = element.value().name();
    if is_boilerplate(element) {
        return;
    }
    output.push('<');
    output.push_str(name);
    for (attribute, value) in &element.value().attrs {
        let name = attribute.local.as_ref();
        if is_output_attribute(name) {
            output.push(' ');
            output.push_str(name);
            output.push_str("=\"");
            push_escaped(output, value, true);
            output.push('"');
        }
    }
    output.push('>');

    if !is_void_element(name) {
        for child in element.children() {
            match child.value() {
                Node::Text(text) => push_escaped(output, &text.text, false),
                Node::Element(_) => {
                    if let Some(child) = ElementRef::wrap(child) {
                        render_element(child, output);
                    }
                }
                _ => {}
            }
        }
        output.push_str("</");
        output.push_str(name);
        output.push('>');
    }
}

fn render_text(element: ElementRef<'_>) -> String {
    let mut output = String::new();
    append_text(element, &mut output);
    normalize_blocks(&output)
}

fn append_text(element: ElementRef<'_>, output: &mut String) {
    let name = element.value().name();
    if is_boilerplate(element) {
        return;
    }
    if name == "br" || is_block_element(name) {
        output.push('\n');
    }
    for child in element.children() {
        match child.value() {
            Node::Text(text) => output.push_str(&text.text),
            Node::Element(_) => {
                if let Some(child) = ElementRef::wrap(child) {
                    append_text(child, output);
                }
            }
            _ => {}
        }
    }
    if is_block_element(name) {
        output.push('\n');
    }
}

fn fragment_text(document: &Html) -> String {
    let mut output = String::new();
    for child in document.root_element().children() {
        match child.value() {
            Node::Text(text) => output.push_str(&text.text),
            Node::Element(_) => {
                if let Some(child) = ElementRef::wrap(child) {
                    append_text(child, &mut output);
                }
            }
            _ => {}
        }
    }
    normalize_blocks(&output)
}

fn normalize_blocks(text: &str) -> String {
    text.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn is_boilerplate(element: ElementRef<'_>) -> bool {
    const EXCLUDED_TAGS: &[&str] = &[
        "aside", "button", "footer", "form", "iframe", "nav", "noscript", "script", "style", "svg",
        "dialog",
    ];
    const EXCLUDED_HINTS: &[&str] = &[
        "advert",
        "banner",
        "breadcrumb",
        "comment",
        "cookie",
        "consent",
        "menu",
        "modal",
        "newsletter",
        "pagination",
        "popup",
        "promo",
        "related",
        "share",
        "sidebar",
        "social",
        "subscribe",
        "widget",
    ];
    if EXCLUDED_TAGS.contains(&element.value().name()) || element.attr("hidden").is_some() {
        return true;
    }
    if element
        .attr("aria-hidden")
        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
    {
        return true;
    }
    let hints = format!(
        "{} {} {}",
        element.attr("id").unwrap_or_default(),
        element.attr("class").unwrap_or_default(),
        element.attr("aria-label").unwrap_or_default()
    )
    .to_ascii_lowercase();
    EXCLUDED_HINTS.iter().any(|hint| hints.contains(hint))
        || hints
            .split(|character: char| !character.is_ascii_alphanumeric())
            .any(|token| matches!(token, "ad" | "ads" | "adslot"))
}

fn is_output_attribute(name: &str) -> bool {
    matches!(
        name,
        "alt" | "class" | "colspan" | "href" | "rowspan" | "src" | "start" | "title" | "value"
    )
}

fn is_void_element(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn is_block_element(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "article"
            | "blockquote"
            | "dd"
            | "div"
            | "dl"
            | "dt"
            | "figcaption"
            | "figure"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "li"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "table"
            | "tr"
            | "ul"
    )
}

fn push_escaped(output: &mut String, text: &str, attribute: bool) {
    for character in text.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' if attribute => output.push_str("&quot;"),
            _ => output.push(character),
        }
    }
}

fn first_meta(document: &Html, queries: &[(&str, &str)]) -> Option<String> {
    queries.iter().find_map(|(attribute, value)| {
        let selector = Selector::parse(&format!("meta[{attribute}=\"{value}\"]")).ok()?;
        document
            .select(&selector)
            .next()
            .and_then(|element| element.value().attr("content"))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

fn select_text(document: &Html, selector: &str) -> Option<String> {
    let selector = Selector::parse(selector).ok()?;
    document
        .select(&selector)
        .next()
        .map(|element| normalize_text(&element.text().collect::<Vec<_>>().join(" ")))
        .filter(|value| !value.is_empty())
}

fn select_attribute(document: &Html, selector: &str, attribute: &str) -> Option<String> {
    let selector = Selector::parse(selector).ok()?;
    document
        .select(&selector)
        .next()
        .and_then(|element| element.value().attr(attribute))
        .map(str::to_owned)
}

fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARTICLE: &str = r#"
        <!doctype html>
        <html lang="en">
          <head>
            <title>Fallback title</title>
            <meta property="og:title" content="An article">
            <meta property="og:description" content="An Open Graph description">
            <meta property="og:site_name" content="Example News">
            <meta property="og:type" content="article">
            <meta name="author" content="Ada">
            <meta name="description" content="A useful page">
            <meta property="article:published_time" content="2025-01-02T03:04:05Z">
            <meta property="article:modified_time" content="2025-02-03T04:05:06Z">
            <link rel="canonical" href="/canonical-article">
            <link rel="icon" href="/favicon.ico">
          </head>
          <body>
            <nav>Navigation should not appear</nav>
            <article>
              <h1>An article</h1>
              <p>This is the main article content with enough words to be recognized by readability.</p>
              <p>It also has another paragraph explaining the details of this article for a useful test.</p>
            </article>
            <footer>Footer should not appear</footer>
          </body>
        </html>
    "#;

    #[test]
    fn extracts_article_and_metadata() {
        let page = extract(
            "https://example.com/article",
            ARTICLE,
            &ExtractOptions { selector: None },
        )
        .expect("article should be extracted");

        assert_eq!(page.title, "An article");
        assert!(page.content.contains("main article content"));
        assert!(!page.content.contains("Navigation should not appear"));
        assert!(!page.content.contains("Footer should not appear"));
        assert_eq!(page.metadata.author.as_deref(), Some("Ada"));
        assert_eq!(page.metadata.language.as_deref(), Some("en"));
        assert_eq!(page.metadata.description.as_deref(), Some("A useful page"));
        assert_eq!(page.metadata.site_name.as_deref(), Some("Example News"));
        assert_eq!(
            page.metadata.published_time.as_deref(),
            Some("2025-01-02T03:04:05Z")
        );
        assert_eq!(
            page.metadata.modified_time.as_deref(),
            Some("2025-02-03T04:05:06Z")
        );
        assert_eq!(
            page.metadata.canonical_url.as_deref(),
            Some("https://example.com/canonical-article")
        );
        assert_eq!(
            page.metadata.favicon.as_deref(),
            Some("https://example.com/favicon.ico")
        );
        assert_eq!(
            page.metadata.open_graph.get("og:type").map(String::as_str),
            Some("article")
        );
    }

    #[test]
    fn selector_limits_the_extracted_content() {
        let page = extract(
            "https://example.com/article",
            ARTICLE,
            &ExtractOptions {
                selector: Some("article"),
            },
        )
        .expect("selector should match");

        assert!(page.content.contains("main article content"));
        assert!(!page.content.contains("Navigation should not appear"));
        assert_eq!(
            page.text,
            "An article\n\nThis is the main article content with enough words to be recognized by readability.\n\nIt also has another paragraph explaining the details of this article for a useful test."
        );
    }

    #[test]
    fn semantic_fallback_prefers_prose_and_removes_nested_boilerplate() {
        let html = r#"
            <main class="article-body">
              <p>This is the first useful paragraph with enough information for readers.</p>
              <div class="related-stories"><a href="/other">Related story</a></div>
              <p>This is the second useful paragraph with additional article detail.</p>
              <nav>Page navigation</nav>
            </main>
        "#;
        let document = Html::parse_document(html);
        let (content, text, score) =
            best_semantic_container(&document).expect("semantic main should be selected");

        assert!(score > 0);
        assert!(content.contains("first useful paragraph"));
        assert!(!content.contains("Related story"));
        assert!(!content.contains("Page navigation"));
        assert!(text.contains("\n\n"));
        assert!(text.contains("second useful paragraph"));
    }

    #[test]
    fn reads_metadata_from_json_ld_when_meta_tags_are_missing() {
        let html = r#"
            <html>
              <head>
                <script type="application/ld+json">
                  {
                    "@context": "https://schema.org",
                    "@graph": [{
                      "@type": "NewsArticle",
                      "headline": "Structured headline",
                      "description": "Structured description",
                      "author": {"@type": "Person", "name": "Grace Hopper"},
                      "publisher": {"@type": "Organization", "name": "Tech Daily"},
                      "datePublished": "2024-03-04",
                      "dateModified": "2024-03-05",
                      "inLanguage": "en"
                    }]
                  }
                </script>
              </head>
              <body>
                <article>
                  <p>This long enough article paragraph contains meaningful details for extraction.</p>
                  <p>Another paragraph provides additional context and enough words to be readable.</p>
                </article>
              </body>
            </html>
        "#;
        let page = extract(
            "https://example.com/news",
            html,
            &ExtractOptions { selector: None },
        )
        .expect("JSON-LD article should be extracted");

        assert_eq!(page.title, "Structured headline");
        assert_eq!(page.metadata.author.as_deref(), Some("Grace Hopper"));
        assert_eq!(
            page.metadata.description.as_deref(),
            Some("Structured description")
        );
        assert_eq!(page.metadata.site_name.as_deref(), Some("Tech Daily"));
        assert_eq!(page.metadata.language.as_deref(), Some("en"));
        assert_eq!(page.metadata.published_time.as_deref(), Some("2024-03-04"));
        assert_eq!(page.metadata.modified_time.as_deref(), Some("2024-03-05"));
    }

    #[test]
    fn drops_cookie_and_ad_blocks_from_semantic_candidates() {
        let html = r#"
            <article>
              <p>This article paragraph has several useful details for the reader.</p>
              <div class="cookie-consent-banner">Accept tracking cookies</div>
              <div id="ad-placement">Advertisement content</div>
              <p>This second paragraph continues the useful article information.</p>
            </article>
        "#;
        let document = Html::parse_document(html);
        let (content, text, _) =
            best_semantic_container(&document).expect("article should be selected");

        assert!(content.contains("several useful details"));
        assert!(!content.contains("tracking cookies"));
        assert!(!content.contains("Advertisement content"));
        assert!(!text.contains("tracking cookies"));
        assert!(!text.contains("Advertisement content"));
    }

    #[test]
    fn does_not_select_a_related_content_container_as_the_article() {
        let html = r#"
            <div class="related-content">
              <p><a href="/one">Related recommendation one</a></p>
              <p><a href="/two">Related recommendation two</a></p>
              <p><a href="/three">Related recommendation three</a></p>
            </div>
            <main>
              <p>This is the main article paragraph with useful information for the reader.</p>
              <p>This is another article paragraph with additional useful information.</p>
            </main>
        "#;
        let document = Html::parse_document(html);
        let (_, text, _) =
            best_semantic_container(&document).expect("main content should be selected");

        assert!(text.contains("main article paragraph"));
        assert!(!text.contains("recommendation"));
    }

    #[test]
    fn reports_invalid_and_unmatched_selectors() {
        assert!(matches!(
            extract(
                "https://example.com/",
                ARTICLE,
                &ExtractOptions {
                    selector: Some("["),
                }
            ),
            Err(ExtractError::InvalidSelector { .. })
        ));
        assert!(matches!(
            extract(
                "https://example.com/",
                ARTICLE,
                &ExtractOptions {
                    selector: Some(".missing"),
                }
            ),
            Err(ExtractError::SelectorNotFound(_))
        ));
    }
}
