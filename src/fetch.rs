use std::time::Duration;

use reqwest::{Client, Url, header::CONTENT_TYPE};

use crate::error::ExtractError;

const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;

#[derive(Clone)]
pub struct Fetcher {
    client: Client,
}

impl Fetcher {
    pub fn new(user_agent: &str, timeout: Duration) -> Result<Self, reqwest::Error> {
        let client = Client::builder()
            .user_agent(user_agent)
            .timeout(timeout)
            .build()?;
        Ok(Self { client })
    }

    pub async fn fetch(&self, input: &str) -> Result<String, ExtractError> {
        let url = Url::parse(input).map_err(|_| ExtractError::InvalidUrl(input.to_owned()))?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err(ExtractError::InvalidUrl(input.to_owned()));
        }

        let mut response = self.client.get(url).send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(ExtractError::HttpStatus {
                status,
                reason: status
                    .canonical_reason()
                    .unwrap_or("unknown status")
                    .to_owned(),
            });
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(ExtractError::ResponseTooLarge {
                limit: MAX_RESPONSE_BYTES,
            });
        }

        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err(ExtractError::ResponseTooLarge {
                    limit: MAX_RESPONSE_BYTES,
                });
            }
            body.extend_from_slice(&chunk);
        }

        Ok(decode_html(&body, content_type.as_deref()))
    }
}

fn decode_html(bytes: &[u8], content_type: Option<&str>) -> String {
    if let Some(label) = content_type.and_then(charset_from_content_type) {
        if let Some(encoding) = encoding_rs::Encoding::for_label(label.as_bytes()) {
            return encoding.decode_with_bom_removal(bytes).0.into_owned();
        }
    }

    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).to_ascii_lowercase();
    if let Some(label) = charset_from_html(&head) {
        if let Some(encoding) = encoding_rs::Encoding::for_label(label.as_bytes()) {
            return encoding.decode_with_bom_removal(bytes).0.into_owned();
        }
    }

    String::from_utf8_lossy(bytes).into_owned()
}

fn charset_from_content_type(value: &str) -> Option<&str> {
    value.split(';').skip(1).find_map(|parameter| {
        let (key, value) = parameter.trim().split_once('=')?;
        (key.trim().eq_ignore_ascii_case("charset")).then(|| value.trim().trim_matches(['\'', '"']))
    })
}

fn charset_from_html(head: &str) -> Option<&str> {
    let index = head.find("charset")?;
    let remainder = head[index + "charset".len()..]
        .trim_start()
        .strip_prefix('=')?
        .trim_start();
    let remainder = remainder.trim_start_matches(['\'', '"']);
    let end = remainder
        .find(|character: char| {
            !character.is_ascii_alphanumeric() && !matches!(character, '-' | '_' | '.')
        })
        .unwrap_or(remainder.len());
    (end > 0).then_some(&remainder[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_charset_in_content_type() {
        assert_eq!(
            charset_from_content_type("text/html; charset=windows-1252"),
            Some("windows-1252")
        );
    }

    #[test]
    fn decodes_a_declared_legacy_charset() {
        let bytes = [0x63, 0x61, 0x66, 0xe9];
        assert_eq!(
            decode_html(&bytes, Some("text/html; charset=windows-1252")),
            "café"
        );
    }
}
