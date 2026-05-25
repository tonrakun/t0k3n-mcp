use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

use super::fs::estimate_tokens;
use super::markdown::{TocEntry, extract_toc};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ConvertDocumentParams {
    #[schemars(description = "Root-relative or absolute path to the document (PDF or DOCX)")]
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConvertDocumentResult {
    pub toc: Vec<TocEntry>,
    pub tmp_path: String,
    pub token_count: usize,
}

pub fn convert_document(root: &Path, params: ConvertDocumentParams) -> anyhow::Result<ConvertDocumentResult> {
    let path = if Path::new(&params.path).is_absolute() {
        Path::new(&params.path).to_path_buf()
    } else {
        root.join(&params.path)
    };

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

    let md = match ext.as_str() {
        "pdf" => convert_pdf(&path)?,
        "docx" => convert_docx(&path)?,
        _ => anyhow::bail!("Unsupported format: {}. Supported: pdf, docx", ext),
    };

    // Write to temp file
    let hash = {
        let mut hasher = Sha256::new();
        hasher.update(path.to_string_lossy().as_bytes());
        hex::encode(&hasher.finalize()[..8])
    };
    let tmp_path = std::env::temp_dir().join(format!("t0k3n-{}.md", hash));
    std::fs::write(&tmp_path, &md)?;

    let toc = extract_toc(&md);
    let token_count = estimate_tokens(&md);
    Ok(ConvertDocumentResult {
        toc,
        tmp_path: tmp_path.to_string_lossy().to_string(),
        token_count,
    })
}

fn convert_pdf(path: &Path) -> anyhow::Result<String> {
    let bytes = std::fs::read(path)?;
    let text = pdf_extract::extract_text_from_mem(&bytes)
        .map_err(|e| anyhow::anyhow!("PDF extraction failed: {}", e))?;
    // Convert plain text to basic markdown
    let md = text
        .lines()
        .map(|l| {
            let trimmed = l.trim();
            if trimmed.is_empty() {
                String::new()
            } else if trimmed.len() < 60 && trimmed.chars().all(|c| c.is_uppercase() || c.is_whitespace()) {
                format!("## {}", trimmed)
            } else {
                trimmed.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(md)
}

fn convert_docx(path: &Path) -> anyhow::Result<String> {
    let bytes = std::fs::read(path)?;
    let docx = docx_rs::read_docx(&bytes)
        .map_err(|e| anyhow::anyhow!("DOCX read failed: {:?}", e))?;

    let mut md = String::new();
    for child in &docx.document.children {
        if let docx_rs::DocumentChild::Paragraph(para) = child {
            let style = para.property.style.as_ref().map(|s| s.val.as_str()).unwrap_or("");
            let text: String = para.children.iter().filter_map(|c| {
                if let docx_rs::ParagraphChild::Run(run) = c {
                    Some(run.children.iter().filter_map(|rc| {
                        if let docx_rs::RunChild::Text(t) = rc {
                            Some(t.text.clone())
                        } else {
                            None
                        }
                    }).collect::<String>())
                } else {
                    None
                }
            }).collect();

            if text.trim().is_empty() {
                md.push('\n');
                continue;
            }

            let line = match style {
                "Heading1" => format!("# {}\n", text),
                "Heading2" => format!("## {}\n", text),
                "Heading3" => format!("### {}\n", text),
                _ => format!("{}\n", text),
            };
            md.push_str(&line);
        }
    }
    Ok(md)
}
