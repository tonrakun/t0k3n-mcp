use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::{
    Router,
    extract::{State, WebSocketUpgrade},
    extract::ws::{Message, WebSocket},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use serde::Serialize;
use tokio::sync::{RwLock, broadcast};

static DASHBOARD_HTML: &str = include_str!("dashboard.html");

#[derive(Debug, Clone, Serialize)]
pub struct ToolCallRecord {
    pub tool: String,
    pub timestamp_ms: u64,
    pub duration_ms: u64,
    pub success: bool,
    pub token_count: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PerToolStat {
    pub calls: u64,
    pub errors: u64,
    pub total_duration_ms: u64,
    pub total_tokens: u64,
}

#[derive(Default, Clone, Serialize)]
pub struct UpdateInfo {
    pub available: Option<String>,
    pub kind: UpdateKind,
}

#[derive(Default, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateKind {
    #[default]
    UpToDate,
    Available,
    Beta,
}

pub struct DashboardState {
    pub version: &'static str,
    start_instant: Instant,
    pub start_ms: u64,
    pub total_calls: AtomicUsize,
    pub error_calls: AtomicUsize,
    total_duration_ms: AtomicU64,
    total_tokens_used: AtomicU64,
    recent_calls: RwLock<Vec<ToolCallRecord>>,
    per_tool_stats: RwLock<HashMap<String, PerToolStat>>,
    update_info: RwLock<UpdateInfo>,
    broadcast: broadcast::Sender<String>,
}

impl DashboardState {
    pub fn new(version: &'static str) -> Arc<Self> {
        let (tx, _) = broadcast::channel(512);
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Arc::new(Self {
            version,
            start_instant: Instant::now(),
            start_ms: now_ms,
            total_calls: AtomicUsize::new(0),
            error_calls: AtomicUsize::new(0),
            total_duration_ms: AtomicU64::new(0),
            total_tokens_used: AtomicU64::new(0),
            recent_calls: RwLock::new(Vec::new()),
            per_tool_stats: RwLock::new(HashMap::new()),
            update_info: RwLock::new(UpdateInfo::default()),
            broadcast: tx,
        })
    }

    pub async fn record_call(&self, tool: String, duration_ms: u64, success: bool, tokens: Option<u64>) {
        let record = ToolCallRecord {
            tool: tool.clone(),
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            duration_ms,
            success,
            token_count: tokens,
        };
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        if !success {
            self.error_calls.fetch_add(1, Ordering::Relaxed);
        }
        self.total_duration_ms.fetch_add(duration_ms, Ordering::Relaxed);
        if let Some(tok) = tokens {
            self.total_tokens_used.fetch_add(tok, Ordering::Relaxed);
        }
        {
            let mut stats = self.per_tool_stats.write().await;
            let entry = stats.entry(tool).or_default();
            entry.calls += 1;
            if !success { entry.errors += 1; }
            entry.total_duration_ms += duration_ms;
            if let Some(tok) = tokens { entry.total_tokens += tok; }
        }
        {
            let mut calls = self.recent_calls.write().await;
            if calls.len() >= 500 {
                calls.remove(0);
            }
            calls.push(record.clone());
        }
        let event = serde_json::json!({ "type": "call", "data": record });
        let _ = self.broadcast.send(event.to_string());
    }

    pub async fn set_update_info(&self, info: UpdateInfo) {
        *self.update_info.write().await = info.clone();
        let event = serde_json::json!({ "type": "update", "data": info });
        let _ = self.broadcast.send(event.to_string());
    }

    async fn snapshot(&self) -> serde_json::Value {
        let total = self.total_calls.load(Ordering::Relaxed);
        let errors = self.error_calls.load(Ordering::Relaxed);
        let total_dur = self.total_duration_ms.load(Ordering::Relaxed);
        let avg_dur = if total > 0 { total_dur / total as u64 } else { 0 };
        let total_tokens = self.total_tokens_used.load(Ordering::Relaxed);
        // Estimated savings: benchmark shows ~87% token reduction on average
        // raw_tokens ≈ tokens_used / 0.13 → tokens_saved = raw - used = used * 6.69
        let tokens_saved = (total_tokens as f64 * 6.69) as u64;

        let calls = self.recent_calls.read().await.clone();
        let update = self.update_info.read().await.clone();

        let per_tool_map = self.per_tool_stats.read().await;
        let mut per_tool: Vec<serde_json::Value> = per_tool_map.iter().map(|(name, stat)| {
            serde_json::json!({
                "tool": name,
                "calls": stat.calls,
                "errors": stat.errors,
                "avg_duration_ms": stat.total_duration_ms.checked_div(stat.calls).unwrap_or(0),
                "total_tokens": stat.total_tokens,
            })
        }).collect();
        per_tool.sort_by(|a, b| {
            b["calls"].as_u64().unwrap_or(0).cmp(&a["calls"].as_u64().unwrap_or(0))
        });

        serde_json::json!({
            "type": "snapshot",
            "version": self.version,
            "start_ms": self.start_ms,
            "uptime_secs": self.start_instant.elapsed().as_secs(),
            "total_calls": total,
            "error_calls": errors,
            "avg_duration_ms": avg_dur,
            "total_tokens_used": total_tokens,
            "tokens_saved": tokens_saved,
            "per_tool": per_tool,
            "recent_calls": calls,
            "update": update,
        })
    }
}

/// A published git tag and the text written when it was created.
/// Annotated tags carry their tag message; lightweight tags fall back to the
/// commit message they point at.
#[derive(Debug, Clone, Serialize)]
pub struct ReleaseNote {
    pub tag: String,
    pub date: String,
    pub body: String,
}

/// Fetches T0K3N-MCP's own GitHub releases (newest first), regardless of which
/// project directory the dashboard's `--root` points at — this panel shows the
/// tool's changelog, not the analyzed project's tags.
async fn fetch_releases() -> Vec<ReleaseNote> {
    let url = format!("https://api.github.com/repos/{}/releases", crate::update::GITHUB_REPO);

    let client = match reqwest::Client::builder()
        .user_agent(format!("t0k3n/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(8))
        .build()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let resp = match client.get(&url).send().await.and_then(|r| r.error_for_status()) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let body = match resp.text().await {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };
    let json: serde_json::Value = match serde_json::from_str(&body) {
        Ok(j) => j,
        Err(_) => return Vec::new(),
    };

    json.as_array()
        .into_iter()
        .flatten()
        .filter(|r| !r["draft"].as_bool().unwrap_or(false))
        .filter_map(|r| {
            let tag = r["tag_name"].as_str()?.to_string();
            let date = r["published_at"].as_str().unwrap_or("").chars().take(10).collect();
            let body = r["body"].as_str().unwrap_or("").trim().to_string();
            Some(ReleaseNote { tag, date, body })
        })
        .collect()
}

pub async fn run(state: Arc<DashboardState>, port: u16) {
    let app = Router::new()
        .route("/", get(serve_html))
        .route("/ws", get(ws_handler))
        .route("/api/state", get(api_state))
        .route("/api/releases", get(api_releases))
        .with_state(state);

    let addr = format!("127.0.0.1:{port}");
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("Dashboard: failed to bind {addr}: {e}");
            return;
        }
    };
    tracing::info!("Dashboard: http://127.0.0.1:{port}");
    axum::serve(listener, app).await.ok();
}

async fn serve_html() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

async fn api_state(State(state): State<Arc<DashboardState>>) -> impl IntoResponse {
    axum::Json(state.snapshot().await)
}

async fn api_releases() -> impl IntoResponse {
    axum::Json(fetch_releases().await)
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<DashboardState>>,
) -> Response {
    ws.on_upgrade(|socket| ws_conn(socket, state))
}

async fn ws_conn(mut socket: WebSocket, state: Arc<DashboardState>) {
    let snap = state.snapshot().await.to_string();
    if socket.send(Message::Text(snap.into())).await.is_err() {
        return;
    }
    let mut rx = state.broadcast.subscribe();
    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(msg) => {
                        if socket.send(Message::Text(msg.into())).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
            msg = socket.recv() => {
                if msg.is_none() { break; }
            }
        }
    }
}
