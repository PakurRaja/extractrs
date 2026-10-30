use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Chunk {
    pub index: usize,
    pub content: String,
}

pub fn chunk_markdown(markdown: &str, size: usize, overlap: usize) -> anyhow::Result<Vec<Chunk>> {
    anyhow::ensure!(size > 0, "chunk size must be greater than zero");
    anyhow::ensure!(
        overlap < size,
        "chunk overlap must be smaller than chunk size"
    );

    let characters: Vec<char> = markdown.chars().collect();
    if characters.is_empty() {
        return Ok(Vec::new());
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    while start < characters.len() {
        let limit = start.saturating_add(size).min(characters.len());
        let mut end = limit;
        if limit < characters.len() {
            let minimum_boundary = start + size.saturating_mul(3) / 4;
            if let Some(boundary) = (minimum_boundary..limit)
                .rev()
                .find(|&index| characters[index].is_whitespace())
            {
                end = boundary;
            }
        }
        if end == start {
            end = limit;
        }
        let content: String = characters[start..end].iter().collect();
        let content = content.trim().to_owned();
        if !content.is_empty() {
            chunks.push(Chunk {
                index: chunks.len(),
                content,
            });
        }
        if end == characters.len() {
            break;
        }

        let next = end.saturating_sub(overlap);
        start = if next <= start { end } else { next };
        while start < end && characters[start].is_whitespace() {
            start += 1;
        }
    }
    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_at_words_and_preserves_requested_overlap() {
        let chunks = chunk_markdown("alpha beta gamma delta epsilon", 12, 4).unwrap();
        assert!(chunks.len() > 1);
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.content.chars().count() <= 12)
        );
        assert!(chunks[0].content.ends_with("beta"));
        assert!(chunks[1].content.starts_with("beta"));
    }

    #[test]
    fn chunks_unicode_without_splitting_codepoints() {
        let chunks = chunk_markdown("猫犬鳥魚猫犬鳥魚", 3, 1).unwrap();
        assert_eq!(
            chunks
                .iter()
                .map(|chunk| chunk.content.chars().count())
                .collect::<Vec<_>>(),
            vec![3, 3, 3, 2]
        );
    }

    #[test]
    fn rejects_invalid_chunk_parameters() {
        assert!(chunk_markdown("text", 0, 0).is_err());
        assert!(chunk_markdown("text", 4, 4).is_err());
    }
}
