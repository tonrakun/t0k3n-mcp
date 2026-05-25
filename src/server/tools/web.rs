use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::fs::estimate_tokens;
use super::markdown::{TocEntry, extract_sections, extract_toc};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FetchWebpageParams {
    #[schemars(description = "URL to fetch")]
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FetchWebpageResult {
    pub toc: Vec<TocEntry>,
    pub token_count: usize,
    pub cached: bool,
}

pub async fn fetch_webpage(
    params: FetchWebpageParams,
    cache: Arc<Mutex<HashMap<String, String>>>,
) -> anyhow::Result<FetchWebpageResult> {
    let cached = {
        let lock = cache.lock().unwrap();
        lock.contains_key(&params.url)
    };

    let md = if cached {
        let lock = cache.lock().unwrap();
        lock.get(&params.url).cloned().unwrap()
    } else {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent("t0k3n-mcp/0.1")
            .build()?;
        let resp = client.get(&params.url).send().await?;
        let html = resp.text().await?;
        let converter = htmd::HtmlToMarkdown::new();
        let md = converter.convert(&html).unwrap_or_else(|_| html.clone());
        {
            let mut lock = cache.lock().unwrap();
            lock.insert(params.url.clone(), md.clone());
        }
        md
    };

    let toc = extract_toc(&md);
    let token_count = estimate_tokens(&md);
    Ok(FetchWebpageResult { toc, token_count, cached })
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadWebpageSectionParams {
    #[schemars(description = "URL (must be fetched with fetch_webpage first)")]
    pub url: String,
    #[schemars(description = "List of heading anchors to extract")]
    pub anchors: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadWebpageSectionResult {
    pub sections: Vec<super::markdown::SectionContent>,
    pub token_count: usize,
}

pub fn read_webpage_section(
    params: ReadWebpageSectionParams,
    cache: Arc<Mutex<HashMap<String, String>>>,
) -> anyhow::Result<ReadWebpageSectionResult> {
    let lock = cache.lock().unwrap();
    let md = lock
        .get(&params.url)
        .ok_or_else(|| anyhow::anyhow!("URL not cached. Call fetch_webpage first."))?
        .clone();
    drop(lock);

    let sections = extract_sections(&md, &params.anchors);
    let json = serde_json::to_string(&sections).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadWebpageSectionResult { sections, token_count })
}
