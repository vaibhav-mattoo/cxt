use tiktoken_rs::CoreBPE;

/// Wraps a tiktoken cl100k_base encoder (GPT-4 / Claude approximation).
/// Falls back to byte estimation if the encoder fails to load.
pub struct TokenCounter {
    bpe: Option<CoreBPE>,
}

impl TokenCounter {
    pub fn new() -> Self {
        Self {
            bpe: tiktoken_rs::cl100k_base().ok(),
        }
    }

    /// Count BPE tokens in `text`. Falls back to `bytes.len() / 4` if encoder unavailable.
    pub fn count(&self, text: &str) -> usize {
        match &self.bpe {
            Some(bpe) => bpe.encode_with_special_tokens(text).len(),
            None => text.len() / 4,
        }
    }
}

/// Fast approximation: source code averages ~4 bytes per token.
pub fn estimate_from_bytes(byte_count: u64) -> usize {
    (byte_count / 4) as usize
}

/// Format a token count with comma separators: 47832 → "47,832".
pub fn format_count(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}
