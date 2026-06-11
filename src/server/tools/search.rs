use std::path::Path;
use std::process::Command;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::code::{ReadCodeBodyParams, ReadCodeSkeletonParams, read_code_body, read_code_skeleton};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SemanticSearchParams {
    #[schemars(description = "Root-relative path to the code file to search")]
    pub path: String,
    #[schemars(description = "Natural language query describing what you are looking for")]
    pub query: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SemanticSearchItem {
    pub id: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SemanticSearchResult {
    pub items: Vec<SemanticSearchItem>,
    pub token_count: usize,
}

pub fn semantic_search(root: &Path, params: SemanticSearchParams) -> Result<SemanticSearchResult, String> {
    // Step 1: skeleton取得
    let skeleton_result = read_code_skeleton(root, ReadCodeSkeletonParams {
        path: params.path.clone(),
        include_blocks: Some(false),
    })
    .map_err(|e| format!("スケルトン取得失敗: {e}"))?;

    if skeleton_result.skeleton.is_empty() {
        return Ok(SemanticSearchResult { items: vec![], token_count: 1 });
    }

    let skeleton_json = serde_json::to_string_pretty(&skeleton_result.skeleton)
        .map_err(|e| e.to_string())?;

    // Step 2: claude CLIサブプロセスで関連IDを特定
    let prompt = format!(
        "以下はコードファイルのスケルトン（関数・クラス一覧）です。\n\
         クエリ: \"{query}\"\n\n\
         スケルトン:\n{skeleton}\n\n\
         クエリに最も関連する関数・ブロックのIDをJSON配列で返してください。\n\
         例: [\"fn:10-25\", \"method:40-60\"]\n\
         IDのみを含むJSON配列だけを出力し、説明文は不要です。",
        query = params.query,
        skeleton = skeleton_json,
    );

    let output = Command::new("claude")
        .args(["-p", &prompt])
        .output()
        .map_err(|e| {
            format!(
                "claude コマンド実行失敗: {e}。\
                 Claude Code CLI がインストール・認証済みか確認してください。"
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("claude 実行エラー: {stderr}"));
    }

    let response = String::from_utf8_lossy(&output.stdout);

    // Step 3: レスポンスからIDリストを抽出
    let ids = extract_ids(&response);
    if ids.is_empty() {
        return Ok(SemanticSearchResult { items: vec![], token_count: 1 });
    }

    // Step 4: 該当IDの本文を返却
    let body_result = read_code_body(root, ReadCodeBodyParams {
        path: params.path,
        ids,
    })
    .map_err(|e| format!("本文取得失敗: {e}"))?;

    let items = body_result
        .items
        .into_iter()
        .map(|item| SemanticSearchItem { id: item.id, content: item.content })
        .collect();

    Ok(SemanticSearchResult {
        items,
        token_count: body_result.token_count,
    })
}

/// レスポンス文字列中の最初のJSON配列を抽出する
fn extract_ids(response: &str) -> Vec<String> {
    let start = response.find('[');
    let end = response.rfind(']');
    if let (Some(s), Some(e)) = (start, end)
        && let Ok(ids) = serde_json::from_str::<Vec<String>>(&response[s..=e]) {
            return ids;
        }
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_ids_clean() {
        let response = r#"["fn:10-25", "method:40-60"]"#;
        assert_eq!(extract_ids(response), vec!["fn:10-25", "method:40-60"]);
    }

    #[test]
    fn test_extract_ids_with_prose() {
        let response = r#"関連するIDは以下です:\n["fn:10-25"]\nです。"#;
        assert_eq!(extract_ids(response), vec!["fn:10-25"]);
    }

    #[test]
    fn test_extract_ids_empty() {
        assert_eq!(extract_ids("IDが見つかりません"), Vec::<String>::new());
    }
}
