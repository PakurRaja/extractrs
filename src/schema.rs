use std::collections::BTreeMap;

use anyhow::{Context, bail};
use serde_json::{Map, Value};

use crate::extract::ExtractedPage;

pub type FieldSchema = BTreeMap<String, String>;

pub fn parse_schema(input: &str) -> anyhow::Result<FieldSchema> {
    let value: Value = serde_json::from_str(input).context("schema must be valid JSON")?;
    let fields = if value.get("type").and_then(Value::as_str) == Some("object") {
        value
            .get("properties")
            .and_then(Value::as_object)
            .context("JSON Schema form requires an object 'properties' map")?
    } else {
        value
            .as_object()
            .context("schema must be a JSON object mapping field names to types")?
    };

    if fields.is_empty() {
        bail!("schema must define at least one field");
    }

    let mut schema = FieldSchema::new();
    for (field, definition) in fields {
        let kind =
            match definition {
                Value::String(kind) => kind.as_str(),
                Value::Object(definition) => definition
                    .get("type")
                    .and_then(Value::as_str)
                    .context(format!(
                        "schema field '{field}' must include a string 'type'"
                    ))?,
                _ => bail!("schema field '{field}' must be a type name or a type object"),
            };
        if !matches!(
            kind,
            "string" | "number" | "integer" | "boolean" | "array" | "object"
        ) {
            bail!("unsupported type '{kind}' for schema field '{field}'");
        }
        schema.insert(field.clone(), kind.to_owned());
    }
    Ok(schema)
}

pub fn extract_fields(page: &ExtractedPage, schema: &FieldSchema) -> Value {
    let mut output = Map::new();
    for (field, kind) in schema {
        let source = find_source_value(page, field);
        output.insert(field.clone(), coerce(source.as_ref(), kind));
    }
    Value::Object(output)
}

fn find_source_value(page: &ExtractedPage, field: &str) -> Option<Value> {
    let key = normalize_key(field);
    if let Some(value) = known_page_value(page, &key) {
        return Some(Value::String(value));
    }
    if let Some((_, value)) = page
        .html_fields
        .iter()
        .find(|(name, _)| normalize_key(name) == key)
    {
        return Some(Value::String(value.clone()));
    }
    page.structured_data
        .iter()
        .find_map(|value| find_json_key(value, &key))
        .or_else(|| {
            page.metadata
                .open_graph
                .iter()
                .find(|(name, _)| normalize_key(name).trim_start_matches("og") == key)
                .map(|(_, value)| Value::String(value.clone()))
        })
}

fn known_page_value(page: &ExtractedPage, key: &str) -> Option<String> {
    match key {
        "title" | "headline" | "name" => Some(page.title.clone()),
        "url" | "sourceurl" => Some(page.url.clone()),
        "content" | "markdown" => Some(page.content.clone()),
        "text" | "body" | "articlebody" => Some(page.text.clone()),
        "author" => page.metadata.author.clone(),
        "description" | "summary" => page.metadata.description.clone(),
        "sitename" | "publisher" => page.metadata.site_name.clone(),
        "language" | "inlanguage" => page.metadata.language.clone(),
        "publishedtime" | "datepublished" | "published" => page.metadata.published_time.clone(),
        "modifiedtime" | "datemodified" | "modified" => page.metadata.modified_time.clone(),
        "canonicalurl" => page.metadata.canonical_url.clone(),
        "favicon" => page.metadata.favicon.clone(),
        _ => None,
    }
}

fn find_json_key(value: &Value, key: &str) -> Option<Value> {
    match value {
        Value::Object(object) => object
            .iter()
            .find_map(|(name, value)| (normalize_key(name) == key).then(|| value.clone()))
            .or_else(|| object.values().find_map(|value| find_json_key(value, key))),
        Value::Array(values) => values.iter().find_map(|value| find_json_key(value, key)),
        _ => None,
    }
}

fn normalize_key(key: &str) -> String {
    key.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn coerce(value: Option<&Value>, kind: &str) -> Value {
    let Some(value) = value else {
        return empty_value(kind);
    };
    match kind {
        "string" => match value {
            Value::String(value) => Value::String(value.clone()),
            Value::Number(_) | Value::Bool(_) => Value::String(value.to_string()),
            _ => Value::Null,
        },
        "number" => value
            .as_f64()
            .or_else(|| value.as_str().and_then(|value| value.trim().parse().ok()))
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        "integer" => value
            .as_i64()
            .or_else(|| value.as_str().and_then(|value| value.trim().parse().ok()))
            .map(|value| Value::Number(value.into()))
            .unwrap_or(Value::Null),
        "boolean" => value
            .as_bool()
            .or_else(|| {
                value
                    .as_str()
                    .and_then(|value| match value.trim().to_ascii_lowercase().as_str() {
                        "true" | "yes" | "1" => Some(true),
                        "false" | "no" | "0" => Some(false),
                        _ => None,
                    })
            })
            .map(Value::Bool)
            .unwrap_or(Value::Null),
        "array" if value.is_array() => value.clone(),
        "object" if value.is_object() => value.clone(),
        _ => empty_value(kind),
    }
}

fn empty_value(kind: &str) -> Value {
    match kind {
        "string" => Value::Null,
        "array" => Value::Array(Vec::new()),
        "object" => Value::Object(Map::new()),
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flat_and_json_schema_forms() {
        assert_eq!(
            parse_schema(r#"{"title":"string","price":"number"}"#).unwrap(),
            BTreeMap::from([
                ("price".to_owned(), "number".to_owned()),
                ("title".to_owned(), "string".to_owned())
            ])
        );
        assert_eq!(
            parse_schema(r#"{"type":"object","properties":{"price":{"type":"number"}}}"#).unwrap()
                ["price"],
            "number"
        );
    }

    #[test]
    fn rejects_invalid_schema_types() {
        assert!(parse_schema(r#"{"thing":"magic"}"#).is_err());
        assert!(parse_schema("[]").is_err());
    }

    #[test]
    fn coerces_structured_html_fields_and_leaves_missing_values_explicit() {
        let page = crate::extract::extract(
            "https://example.com/product",
            r#"
                <html><body>
                  <h1>Widget</h1>
                  <span itemprop="price">19.95</span>
                  <meta itemprop="inStock" content="true">
                </body></html>
            "#,
            &crate::extract::ExtractOptions { selector: None },
        )
        .unwrap();
        let schema = parse_schema(
            r#"{"title":"string","price":"number","inStock":"boolean","missing":"string"}"#,
        )
        .unwrap();
        let result = extract_fields(&page, &schema);

        assert_eq!(result["title"], "Widget");
        assert_eq!(result["price"], 19.95);
        assert_eq!(result["inStock"], true);
        assert!(result["missing"].is_null());
    }
}
