use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::fs::estimate_tokens;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CompressTextParams {
    #[schemars(description = "Text to compress")]
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CompressTextResult {
    pub compressed: String,
    pub original_tokens: usize,
    pub compressed_tokens: usize,
    pub ratio: f64,
}

pub fn compress_text(params: CompressTextParams) -> CompressTextResult {
    let original_tokens = estimate_tokens(&params.text);
    let compressed = do_compress(&params.text);
    let compressed_tokens = estimate_tokens(&compressed);
    let ratio = if original_tokens > 0 {
        1.0 - compressed_tokens as f64 / original_tokens as f64
    } else {
        0.0
    };
    CompressTextResult {
        compressed,
        original_tokens,
        compressed_tokens,
        ratio,
    }
}

fn do_compress(text: &str) -> String {
    // Remove excessive blank lines
    let blank_re = Regex::new(r"\n{3,}").unwrap();
    let text = blank_re.replace_all(text, "\n\n");

    // Remove trailing whitespace per line
    let trailing_re = Regex::new(r"[ \t]+$").unwrap();
    let text = text
        .lines()
        .map(|l| trailing_re.replace(l, "").to_string())
        .collect::<Vec<_>>()
        .join("\n");

    // Remove HTML comments
    let comment_re = Regex::new(r"<!--.*?-->").unwrap();
    let text = comment_re.replace_all(&text, "");

    text.trim().to_string()
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CountTokensParams {
    #[schemars(description = "Text to count tokens for")]
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CountTokensResult {
    pub token_count: usize,
    pub char_count: usize,
    pub line_count: usize,
}

pub fn count_tokens(params: CountTokensParams) -> CountTokensResult {
    CountTokensResult {
        token_count: estimate_tokens(&params.text),
        char_count: params.text.chars().count(),
        line_count: params.text.lines().count(),
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct CheckBudgetParams {
    #[schemars(description = "Total token budget for the session")]
    pub budget: i64,
    #[schemars(description = "Tokens already used in this session")]
    pub used: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CheckBudgetResult {
    pub remaining: i64,
    pub used_percent: f64,
    pub strategy: String,
    pub recommendations: Vec<String>,
}

pub fn check_budget(params: CheckBudgetParams) -> CheckBudgetResult {
    let remaining = params.budget - params.used;
    let used_percent = if params.budget > 0 {
        params.used as f64 / params.budget as f64 * 100.0
    } else {
        0.0
    };

    let (strategy, recommendations) = if used_percent < 50.0 {
        (
            "normal".to_string(),
            vec![
                "Budget is healthy. Normal reading patterns are fine.".to_string(),
            ],
        )
    } else if used_percent < 75.0 {
        (
            "conservative".to_string(),
            vec![
                "Use read_code_skeleton before read_code_body.".to_string(),
                "Use read_markdown_toc before read_markdown_section.".to_string(),
                "Prefer search_file over reading whole files.".to_string(),
            ],
        )
    } else if used_percent < 90.0 {
        (
            "aggressive".to_string(),
            vec![
                "Only read specific sections/functions needed.".to_string(),
                "Use compress_text on large inputs.".to_string(),
                "Summarize conversation with summarize_conversation.".to_string(),
                "Avoid re-reading already processed content.".to_string(),
            ],
        )
    } else {
        (
            "critical".to_string(),
            vec![
                "Budget nearly exhausted. Wrap up the current task.".to_string(),
                "Avoid any large file reads.".to_string(),
                "Use only targeted searches and specific ID lookups.".to_string(),
            ],
        )
    };

    CheckBudgetResult {
        remaining,
        used_percent,
        strategy,
        recommendations,
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SummarizeConversationParams {
    #[schemars(description = "Conversation text to summarize")]
    pub text: String,
    #[schemars(description = "Maximum summary length in tokens (default: 500)")]
    pub max_tokens: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SummarizeConversationResult {
    pub summary: String,
    pub token_count: usize,
}

pub fn summarize_conversation(params: SummarizeConversationParams) -> SummarizeConversationResult {
    let max_tokens = params.max_tokens.unwrap_or(500);
    // Simple extractive summarization: take first N tokens worth of content.
    // Slice on char boundaries — byte slicing panics on multibyte (CJK) text.
    let target_chars = max_tokens * 4;
    let cut = params.text
        .char_indices()
        .nth(target_chars)
        .map(|(i, _)| i);
    let summary = match cut {
        None => params.text.clone(),
        Some(cut) => {
            let truncated = &params.text[..cut];
            // Find last complete sentence (ASCII and CJK sentence enders)
            let end = truncated
                .rfind(['.', '!', '?', '。', '！', '？'])
                .map(|i| i + truncated[i..].chars().next().map_or(0, char::len_utf8))
                .unwrap_or(cut);
            format!("{}...\n\n[Conversation truncated to fit token budget]", &params.text[..end])
        }
    };
    let token_count = estimate_tokens(&summary);
    SummarizeConversationResult { summary, token_count }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_truncates_multibyte_text_without_panic() {
        // 3-byte chars: byte slicing at char index N would land mid-codepoint
        let text = "これはとても長い日本語の会話です。".repeat(200);
        let result = summarize_conversation(SummarizeConversationParams {
            text,
            max_tokens: Some(10),
        });
        assert!(result.summary.contains("[Conversation truncated"));
        assert!(result.summary.contains('。'));
    }

    #[test]
    fn summarize_returns_short_text_unchanged() {
        let result = summarize_conversation(SummarizeConversationParams {
            text: "short".to_string(),
            max_tokens: Some(500),
        });
        assert_eq!(result.summary, "short");
    }

    #[test]
    fn count_tokens_counts_chars_not_bytes() {
        let result = count_tokens(CountTokensParams { text: "日本語".to_string() });
        assert_eq!(result.char_count, 3);
    }
}
