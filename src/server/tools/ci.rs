use std::path::Path;

use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::security::{rel_display, safe_path};
use super::fs::estimate_tokens;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadCiPipelineParams {
    #[schemars(description = "Path to a specific CI YAML file, or omit to auto-scan workspace root.")]
    pub path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CiJob {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs_on: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<String>,
    pub steps: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub env_vars: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CiWorkflow {
    pub name: String,
    pub triggers: Vec<String>,
    pub jobs: Vec<CiJob>,
}

#[derive(Debug, Serialize)]
pub struct CiPipeline {
    pub path: String,
    pub format: String,
    pub workflows: Vec<CiWorkflow>,
}

#[derive(Debug, Serialize)]
pub struct ReadCiPipelineResult {
    pub pipelines: Vec<CiPipeline>,
    pub token_count: usize,
}

pub fn read_ci_pipeline(root: &Path, params: ReadCiPipelineParams) -> anyhow::Result<ReadCiPipelineResult> {
    let mut pipelines = Vec::new();

    if let Some(ref p) = params.path {
        let full = safe_path(root, p)?;
        if let Some(pipeline) = parse_ci_file(&full, p) {
            pipelines.push(pipeline);
        }
    } else {
        // GitHub Actions
        let workflows_dir = root.join(".github").join("workflows");
        if workflows_dir.exists() {
            let mut entries: Vec<_> = std::fs::read_dir(&workflows_dir)
                .into_iter()
                .flatten()
                .flatten()
                .collect();
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext != "yml" && ext != "yaml" { continue; }
                let rel = rel_display(root, &path);
                if let Some(pipeline) = parse_ci_file(&path, &rel) {
                    pipelines.push(pipeline);
                }
            }
        }
        // GitLab CI
        for name in &[".gitlab-ci.yml", ".gitlab-ci.yaml"] {
            let p = root.join(name);
            if p.exists()
                && let Some(pipeline) = parse_ci_file(&p, name) {
                    pipelines.push(pipeline);
                }
        }
        // CircleCI
        for name in &[".circleci/config.yml", ".circleci/config.yaml"] {
            let p = root.join(name);
            if p.exists()
                && let Some(pipeline) = parse_ci_file(&p, name) {
                    pipelines.push(pipeline);
                }
        }
    }

    let json = serde_json::to_string(&pipelines).unwrap_or_default();
    let token_count = estimate_tokens(&json);
    Ok(ReadCiPipelineResult { pipelines, token_count })
}

fn detect_ci_format(path: &Path) -> Option<&'static str> {
    let s = path.to_string_lossy().replace('\\', "/");
    if s.contains(".github/workflows/") { return Some("github-actions"); }
    if s.ends_with(".gitlab-ci.yml") || s.ends_with(".gitlab-ci.yaml") { return Some("gitlab-ci"); }
    if s.contains(".circleci/config.yml") || s.contains(".circleci/config.yaml") { return Some("circleci"); }
    None
}

fn parse_ci_file(path: &Path, rel: &str) -> Option<CiPipeline> {
    let format = detect_ci_format(path)?;
    let content = std::fs::read_to_string(path).ok()?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&content).ok()?;

    let workflows = match format {
        "github-actions" => parse_github_actions(&doc),
        "gitlab-ci" => parse_gitlab_ci(&doc),
        "circleci" => parse_circleci(&doc),
        _ => return None,
    };

    Some(CiPipeline {
        path: rel.to_string(),
        format: format.to_string(),
        workflows,
    })
}

fn collect_env_var_refs(value: &serde_yaml::Value) -> Vec<String> {
    let s = serde_yaml::to_string(value).unwrap_or_default();
    let re = Regex::new(r"\$\{\{\s*(?:env|secrets|vars)\.(\w+)\s*\}\}").unwrap();
    let mut vars: Vec<String> = re.captures_iter(&s).map(|c| c[1].to_string()).collect();
    vars.dedup();
    vars
}

fn parse_github_actions(doc: &serde_yaml::Value) -> Vec<CiWorkflow> {
    let mut triggers = Vec::new();
    if let Some(on_val) = doc.get("on") {
        match on_val {
            serde_yaml::Value::Sequence(seq) => {
                for item in seq {
                    if let Some(s) = item.as_str() { triggers.push(s.to_string()); }
                }
            }
            serde_yaml::Value::Mapping(map) => {
                for (k, _) in map {
                    if let Some(s) = k.as_str() { triggers.push(s.to_string()); }
                }
            }
            serde_yaml::Value::String(s) => triggers.push(s.clone()),
            _ => {}
        }
    }

    let workflow_name = doc.get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("workflow")
        .to_string();

    let mut jobs = Vec::new();
    if let Some(jobs_map) = doc.get("jobs").and_then(|j| j.as_mapping()) {
        for (job_key, job_val) in jobs_map {
            let job_name = job_key.as_str().unwrap_or("unknown").to_string();
            let runs_on = job_val.get("runs-on")
                .and_then(|r| r.as_str())
                .map(|s| s.to_string());
            let needs = job_val.get("needs").map(|n| match n {
                serde_yaml::Value::String(s) => vec![s.clone()],
                serde_yaml::Value::Sequence(seq) => seq.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect(),
                _ => vec![],
            }).unwrap_or_default();

            let mut steps = Vec::new();
            if let Some(steps_seq) = job_val.get("steps").and_then(|s| s.as_sequence()) {
                for step in steps_seq {
                    if let Some(name) = step.get("name").and_then(|n| n.as_str()) {
                        steps.push(name.to_string());
                    } else if let Some(uses) = step.get("uses").and_then(|u| u.as_str()) {
                        steps.push(format!("uses: {}", uses));
                    } else if let Some(run) = step.get("run").and_then(|r| r.as_str()) {
                        let first = run.lines().next().unwrap_or("(script)").trim();
                        steps.push(format!("run: {}", first));
                    }
                }
            }

            let env_vars = collect_env_var_refs(job_val);
            jobs.push(CiJob { name: job_name, runs_on, needs, steps, env_vars });
        }
    }

    vec![CiWorkflow { name: workflow_name, triggers, jobs }]
}

fn parse_gitlab_ci(doc: &serde_yaml::Value) -> Vec<CiWorkflow> {
    const SPECIAL_KEYS: &[&str] = &[
        "stages", "variables", "image", "services", "before_script",
        "after_script", "cache", "include", "workflow", "default",
    ];

    let mut triggers = Vec::new();
    if let Some(workflow) = doc.get("workflow")
        && let Some(rules) = workflow.get("rules").and_then(|r| r.as_sequence()) {
            for rule in rules {
                if let Some(cond) = rule.get("if").and_then(|r| r.as_str()) {
                    triggers.push(cond.to_string());
                }
            }
        }
    if triggers.is_empty() { triggers.push("push".to_string()); }

    let mut jobs = Vec::new();
    if let Some(map) = doc.as_mapping() {
        for (k, v) in map {
            let name = match k.as_str() { Some(s) => s, None => continue };
            if SPECIAL_KEYS.contains(&name) { continue; }

            let mut steps = Vec::new();
            if let Some(script) = v.get("script").and_then(|s| s.as_sequence()) {
                for line in script {
                    if let Some(s) = line.as_str() { steps.push(s.to_string()); }
                }
            }
            let env_vars = v.get("variables")
                .and_then(|vars| vars.as_mapping())
                .map(|m| m.iter().filter_map(|(k, _)| k.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();

            jobs.push(CiJob {
                name: name.to_string(),
                runs_on: v.get("image").and_then(|i| i.as_str()).map(|s| s.to_string()),
                needs: vec![],
                steps,
                env_vars,
            });
        }
    }

    vec![CiWorkflow { name: "pipeline".to_string(), triggers, jobs }]
}

fn parse_circleci(doc: &serde_yaml::Value) -> Vec<CiWorkflow> {
    let mut all_workflows = Vec::new();

    if let Some(workflows_map) = doc.get("workflows").and_then(|w| w.as_mapping()) {
        for (wf_name, wf_val) in workflows_map {
            let name = wf_name.as_str().unwrap_or("workflow").to_string();
            if name == "version" { continue; }

            let job_names: Vec<String> = wf_val.get("jobs")
                .and_then(|j| j.as_sequence())
                .map(|seq| seq.iter().filter_map(|j| match j {
                    serde_yaml::Value::String(s) => Some(s.clone()),
                    serde_yaml::Value::Mapping(m) => m.keys().next()
                        .and_then(|k| k.as_str().map(|s| s.to_string())),
                    _ => None,
                }).collect())
                .unwrap_or_default();

            let mut jobs = Vec::new();
            if let Some(jobs_map) = doc.get("jobs").and_then(|j| j.as_mapping()) {
                for job_name in &job_names {
                    let Some(job_val) = jobs_map.get(job_name.as_str()) else { continue };

                    let mut steps = Vec::new();
                    if let Some(steps_seq) = job_val.get("steps").and_then(|s| s.as_sequence()) {
                        for step in steps_seq {
                            match step {
                                serde_yaml::Value::String(s) => steps.push(s.clone()),
                                serde_yaml::Value::Mapping(m) => {
                                    if let Some(k) = m.keys().next().and_then(|k| k.as_str()) {
                                        steps.push(k.to_string());
                                    }
                                }
                                _ => {}
                            }
                        }
                    }

                    let runs_on = job_val.get("docker")
                        .and_then(|d| d.as_sequence())
                        .and_then(|s| s.first())
                        .and_then(|f| f.get("image"))
                        .and_then(|i| i.as_str())
                        .map(|s| s.to_string());

                    jobs.push(CiJob { name: job_name.clone(), runs_on, needs: vec![], steps, env_vars: vec![] });
                }
            }

            all_workflows.push(CiWorkflow { name, triggers: vec!["workflow".to_string()], jobs });
        }
    }

    all_workflows
}
