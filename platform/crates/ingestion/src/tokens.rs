//! Counting tokens.
//!
//! Chunk sizes are budgets against a model's context window, so a rough
//! word count is not good enough: Thai has no spaces, and a "500 word" chunk of
//! it can be several thousand tokens. `cl100k_base` is used as one common
//! yardstick across providers — it is not exactly what any of them charges for,
//! but it is close, deterministic, and free to compute.
//!
//! Real accounting always uses the token counts a provider reports back.

use std::sync::OnceLock;

use tiktoken_rs::CoreBPE;

fn encoder() -> Option<&'static CoreBPE> {
    static ENCODER: OnceLock<Option<CoreBPE>> = OnceLock::new();

    ENCODER
        .get_or_init(|| match tiktoken_rs::cl100k_base() {
            Ok(bpe) => Some(bpe),
            Err(e) => {
                tracing::warn!(error = %e, "tokenizer unavailable, falling back to an estimate");
                None
            }
        })
        .as_ref()
}

/// How many tokens this text is worth.
pub fn count(text: &str) -> usize {
    match encoder() {
        Some(bpe) => bpe.encode_ordinary(text).len(),
        None => estimate(text),
    }
}

/// The fallback when the tokenizer cannot be loaded. Deliberately generous for
/// scripts without spaces, so a chunk is too small rather than too large — an
/// over-long chunk is rejected by the provider, an under-long one merely
/// retrieves a little less context.
pub fn estimate(text: &str) -> usize {
    let words = text.split_whitespace().count();
    let chars = text.chars().count();
    words.max(chars / 3).max(1)
}

/// Cut `text` to at most `max_tokens`, on a token boundary.
///
/// Used where a hard ceiling matters more than the tail of the text: a chunk
/// that overflows the window fails the whole request.
pub fn truncate(text: &str, max_tokens: usize) -> String {
    let Some(bpe) = encoder() else {
        // Without a tokenizer, fall back to characters and be conservative.
        return text.chars().take(max_tokens * 3).collect();
    };

    let tokens = bpe.encode_ordinary(text);
    if tokens.len() <= max_tokens {
        return text.to_owned();
    }
    bpe.decode(tokens[..max_tokens].to_vec())
        .unwrap_or_else(|_| text.chars().take(max_tokens * 3).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_english_close_to_word_count() {
        let text = "The Rust programming course runs for twelve weeks.";
        let tokens = count(text);
        assert!(
            (6..=16).contains(&tokens),
            "expected roughly one token per word, got {tokens}"
        );
    }

    #[test]
    fn thai_costs_far_more_than_its_word_count_suggests() {
        // The reason this module exists: splitting on whitespace would call
        // this one word, and a chunk of it would blow the context window.
        let thai = "หลักสูตรนี้ใช้เวลาเรียนสิบสองสัปดาห์";
        assert_eq!(thai.split_whitespace().count(), 1);
        assert!(
            count(thai) > 10,
            "Thai must not be counted as a single token: {}",
            count(thai)
        );
    }

    #[test]
    fn an_empty_string_costs_nothing() {
        assert_eq!(count(""), 0);
    }

    #[test]
    fn counting_is_deterministic() {
        let text = "หลักสูตร Rust ใช้เวลาเรียน 12 สัปดาห์";
        assert_eq!(count(text), count(text));
    }

    #[test]
    fn longer_text_costs_more() {
        let short = "Rust runs for twelve weeks.";
        let long = short.repeat(10);
        assert!(count(&long) > count(short));
    }

    #[test]
    fn truncation_respects_the_ceiling() {
        let long = "This sentence exists to be cut short. ".repeat(100);
        let cut = truncate(&long, 20);

        assert!(count(&cut) <= 20, "truncation must respect the limit");
        assert!(long.starts_with(&cut[..cut.len().min(20)]));
    }

    #[test]
    fn short_text_is_left_alone_by_truncation() {
        let text = "already short";
        assert_eq!(truncate(text, 100), text);
    }

    #[test]
    fn truncating_thai_does_not_produce_broken_characters() {
        let thai = "หลักสูตร Rust ใช้เวลาเรียน 12 สัปดาห์ ".repeat(20);
        let cut = truncate(&thai, 30);
        // Decoding from tokens can only produce valid UTF-8, which is the point
        // of cutting on a token boundary rather than a byte one.
        assert!(cut.chars().count() > 0);
    }

    #[test]
    fn the_fallback_estimate_is_generous_for_spaceless_scripts() {
        let thai = "หลักสูตรนี้ใช้เวลาเรียนสิบสองสัปดาห์";
        assert!(estimate(thai) > 5);
        assert_eq!(estimate(""), 1, "never zero, so a budget always advances");
    }
}
