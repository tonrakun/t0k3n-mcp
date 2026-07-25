//! Tool handlers, split by the same categories `help()` reports.
//!
//! Each module contributes one `ToolRouter` via `#[tool_router]`; [`tool_router`]
//! merges them into the single router the server serves. Splitting them keeps any
//! one file readable — the previous single 2.8k-line `server/mod.rs` was this
//! project's own largest file, which is a poor advertisement for a token-saving
//! server.

use crate::server::*;

mod analysis;
mod cmd;
mod debug;
mod file;
mod git;
mod log;
mod memory;
mod notebook;
mod schema;
mod session;
mod task;
mod test;
mod text;
mod web;
mod write;

impl T0k3nServer {
    /// The complete tool router: every category merged, in `help()` order.
    ///
    /// Capability gates (`--enable-writes`, `--disable-commands`, `--tools`) are
    /// applied afterwards in [`T0k3nServer::new`], not here.
    pub(crate) fn tool_router() -> ToolRouter<Self> {
        Self::file_router()
            + Self::write_router()
            + Self::git_router()
            + Self::schema_router()
            + Self::web_router()
            + Self::notebook_router()
            + Self::test_router()
            + Self::log_router()
            + Self::text_router()
            + Self::memory_router()
            + Self::task_router()
            + Self::session_router()
            + Self::analysis_router()
            + Self::cmd_router()
            + Self::debug_router()
    }
}
