use quick_html2md::html_to_markdown as convert_html;

use crate::extract::ExtractedPage;

pub fn markdown(page: &ExtractedPage) -> String {
    let mut output = String::new();
    let first_heading = page
        .content
        .lines()
        .find(|line| !line.trim().is_empty())
        .filter(|line| line.trim_start().starts_with('#'))
        .map(|line| line.trim_start_matches('#').trim());
    if !page.title.trim().is_empty()
        && !first_heading.is_some_and(|heading| heading.eq_ignore_ascii_case(page.title.trim()))
    {
        output.push_str("# ");
        output.push_str(page.title.trim());
        output.push_str("\n\n");
    }
    output.push_str(page.content.trim());
    output.push('\n');
    output
}

pub fn html_to_markdown(html: &str) -> String {
    convert_html(html).trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avoids_repeating_an_existing_title_heading() {
        let page = ExtractedPage {
            title: "Article title".to_owned(),
            content: "# Article title\n\nBody text".to_owned(),
            url: "https://example.com".to_owned(),
            metadata: crate::extract::PageMetadata {
                author: None,
                description: None,
                site_name: None,
                language: None,
                published_time: None,
                modified_time: None,
                canonical_url: None,
                favicon: None,
                open_graph: Default::default(),
            },
            text: "Body text".to_owned(),
            structured_data: Vec::new(),
            html_fields: std::collections::BTreeMap::new(),
        };
        assert_eq!(markdown(&page), "# Article title\n\nBody text\n");
    }

    #[test]
    fn preserves_links_lists_and_code_blocks() {
        let markdown = html_to_markdown(
            r#"
                <h2>Guide</h2>
                <p>Read the <a href="https://example.com/docs">documentation</a>.</p>
                <ul><li>First</li><li>Second</li></ul>
                <pre><code class="language-rust">fn main() { println!("hello"); }</code></pre>
            "#,
        );

        assert!(markdown.contains("## Guide"));
        assert!(markdown.contains("[documentation](https://example.com/docs)"));
        assert!(markdown.contains("- First"));
        assert!(markdown.contains("- Second"));
        assert!(markdown.contains("```rust"));
        assert!(markdown.contains("fn main()"));
    }
}
