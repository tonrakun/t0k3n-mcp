//! Delta reads — a per-session read ledger that suppresses re-sending
//! content the LLM has already received.
//!
//! Keyed by (tool name + canonical params). On a repeat call:
//! - identical rendered output → tiny "unchanged" response
//! - changed output → unified diff against what was previously sent,
//!   but only when the diff is meaningfully smaller than the full output
//!
//! The ledger lives in memory for the lifetime of the server process
//! (one MCP session). `delta_reset` clears it, e.g. after client-side
//! context compaction loses the originally-read content.

use schemars::JsonSchema;
use serde::Deserialize;
use std::collections::HashMap;

use super::fs::estimate_tokens;

/// Outputs larger than this are never tracked (memory bound).
const MAX_ENTRY_BYTES: usize = 512 * 1024;
/// Hard cap on tracked entries; the ledger is cleared wholesale beyond it.
const MAX_ENTRIES: usize = 512;
/// A diff is only returned when it is at most this fraction (in tenths)
/// of the full output size.
const DIFF_WORTHWHILE_TENTHS: usize = 6;

pub enum Delta {
    /// Send the full output (first read, untracked, or diff not worthwhile).
    Full,
    Unchanged { full_tokens: usize },
    Diff { diff: String, full_tokens: usize },
}

#[derive(Default)]
pub struct ReadLedger {
    entries: HashMap<String, String>,
}

impl ReadLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn check_and_update(&mut self, key: &str, rendered: &str) -> Delta {
        if rendered.len() > MAX_ENTRY_BYTES {
            self.entries.remove(key);
            return Delta::Full;
        }
        let prev = self.entries.get(key);
        let result = match prev {
            Some(prev) if prev == rendered => {
                return Delta::Unchanged { full_tokens: estimate_tokens(rendered) };
            }
            Some(prev) => {
                let diff = unified_diff(prev, rendered);
                if diff.len() * 10 <= rendered.len() * DIFF_WORTHWHILE_TENTHS {
                    Delta::Diff { diff, full_tokens: estimate_tokens(rendered) }
                } else {
                    Delta::Full
                }
            }
            None => Delta::Full,
        };
        if self.entries.len() >= MAX_ENTRIES && !self.entries.contains_key(key) {
            self.entries.clear();
        }
        self.entries.insert(key.to_string(), rendered.to_string());
        result
    }

    /// Clear entries whose key contains `pattern` (all entries when None).
    /// Returns the number of entries removed.
    pub fn clear(&mut self, pattern: Option<&str>) -> usize {
        match pattern {
            None => {
                let n = self.entries.len();
                self.entries.clear();
                n
            }
            Some(p) => {
                let before = self.entries.len();
                self.entries.retain(|k, _| !k.contains(p));
                before - self.entries.len()
            }
        }
    }
}

fn unified_diff(old: &str, new: &str) -> String {
    similar::TextDiff::from_lines(old, new)
        .unified_diff()
        .context_radius(2)
        .header("previous_read", "current")
        .to_string()
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeltaResetParams {
    #[schemars(description = "Substring filter on ledger keys (e.g. a file path). Omit to clear the entire ledger.")]
    pub pattern: Option<String>,
}

// ─── cross-tool content ledger (gen3) ─────────────────────────────────────────
//
// The ReadLedger above is keyed by tool+params, so it only catches a *repeat of
// the same call*. This second ledger is keyed by file content (path + line range)
// so that the same body sent by one tool (e.g. read_context_pack) and then
// re-requested by another (read_code_body) is replaced with a tiny reference
// stub. Invalidation: an entry is only reused when the file's mtime is unchanged
// AND the content hash still matches, so a line-range shift after an edit never
// produces a stale reference.

/// Hard cap on tracked content entries (memory bound).
const CONTENT_MAX_ENTRIES: usize = 1024;

pub enum ContentDedup {
    /// Not seen (or invalidated) — send the full content; it has been recorded.
    Fresh,
    /// Identical content was already sent this session under `reference`.
    AlreadySent { reference: String, full_tokens: usize },
}

struct ContentEntry {
    mtime: u64,
    hash: u64,
    tokens: usize,
}

#[derive(Default)]
pub struct ContentLedger {
    entries: HashMap<String, ContentEntry>,
}

impl ContentLedger {
    pub fn new() -> Self {
        Self::default()
    }

    fn key(path: &str, id: &str) -> String {
        format!("{path}#{id}")
    }

    /// Record (or match) a body chunk. Returns `AlreadySent` only when an entry
    /// for the same path+range exists with the same mtime and content hash.
    pub fn dedup(&mut self, path: &str, id: &str, content: &str, mtime: u64) -> ContentDedup {
        let key = Self::key(path, id);
        let hash = hash_str(content);
        let tokens = estimate_tokens(content);

        if let Some(e) = self.entries.get(&key)
            && e.mtime == mtime
            && e.hash == hash
        {
            let range = id.rsplit_once(':').map(|(_, r)| r).unwrap_or(id);
            return ContentDedup::AlreadySent {
                reference: format!("{path}:{range}"),
                full_tokens: e.tokens,
            };
        }

        if self.entries.len() >= CONTENT_MAX_ENTRIES && !self.entries.contains_key(&key) {
            self.entries.clear();
        }
        self.entries.insert(key, ContentEntry { mtime, hash, tokens });
        ContentDedup::Fresh
    }

    /// Clear entries whose key contains `pattern` (all entries when None).
    pub fn clear(&mut self, pattern: Option<&str>) -> usize {
        match pattern {
            None => {
                let n = self.entries.len();
                self.entries.clear();
                n
            }
            Some(p) => {
                let before = self.entries.len();
                self.entries.retain(|k, _| !k.contains(p));
                before - self.entries.len()
            }
        }
    }
}

fn hash_str(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_read_is_full_repeat_is_unchanged() {
        let mut l = ReadLedger::new();
        assert!(matches!(l.check_and_update("k", "abc"), Delta::Full));
        assert!(matches!(l.check_and_update("k", "abc"), Delta::Unchanged { .. }));
    }

    #[test]
    fn small_change_in_large_content_yields_diff() {
        let mut l = ReadLedger::new();
        let old: String = (0..100).map(|i| format!("line {i}\n")).collect();
        let new = old.replace("line 50", "line fifty");
        assert!(matches!(l.check_and_update("k", &old), Delta::Full));
        match l.check_and_update("k", &new) {
            Delta::Diff { diff, .. } => {
                assert!(diff.contains("line fifty"));
                assert!(diff.contains("-line 50"));
            }
            _ => panic!("expected diff"),
        }
        // ledger now holds the new content
        assert!(matches!(l.check_and_update("k", &new), Delta::Unchanged { .. }));
    }

    #[test]
    fn total_rewrite_falls_back_to_full() {
        let mut l = ReadLedger::new();
        l.check_and_update("k", "aaa\nbbb\nccc");
        assert!(matches!(l.check_and_update("k", "xxx\nyyy\nzzz"), Delta::Full));
    }

    #[test]
    fn clear_by_pattern() {
        let mut l = ReadLedger::new();
        l.check_and_update("read_code_skeleton:{\"path\":\"a.rs\"}", "1");
        l.check_and_update("read_code_skeleton:{\"path\":\"b.rs\"}", "2");
        assert_eq!(l.clear(Some("a.rs")), 1);
        assert_eq!(l.clear(None), 1);
    }

    #[test]
    fn content_ledger_stubs_cross_tool_repeat() {
        let mut l = ContentLedger::new();
        let body = "fn f() {\n    work();\n}";
        // first send (e.g. from read_context_pack) records it
        assert!(matches!(l.dedup("a.rs", "function:1-3", body, 100), ContentDedup::Fresh));
        // re-request (e.g. from read_code_body) with same mtime → stub
        match l.dedup("a.rs", "function:1-3", body, 100) {
            ContentDedup::AlreadySent { reference, .. } => assert_eq!(reference, "a.rs:1-3"),
            _ => panic!("expected AlreadySent"),
        }
    }

    #[test]
    fn content_ledger_invalidates_on_mtime_change() {
        let mut l = ContentLedger::new();
        let body = "fn f() {}";
        assert!(matches!(l.dedup("a.rs", "function:1-1", body, 100), ContentDedup::Fresh));
        // file edited (mtime changed) → no stale reference even if content matches
        assert!(matches!(l.dedup("a.rs", "function:1-1", body, 200), ContentDedup::Fresh));
    }

    #[test]
    fn content_ledger_invalidates_on_content_change() {
        let mut l = ContentLedger::new();
        assert!(matches!(l.dedup("a.rs", "function:1-1", "old", 100), ContentDedup::Fresh));
        assert!(matches!(l.dedup("a.rs", "function:1-1", "new", 100), ContentDedup::Fresh));
    }

    #[test]
    fn content_ledger_clear_by_pattern() {
        let mut l = ContentLedger::new();
        l.dedup("a.rs", "function:1-2", "x", 1);
        l.dedup("b.rs", "function:1-2", "y", 1);
        assert_eq!(l.clear(Some("a.rs")), 1);
        assert_eq!(l.clear(None), 1);
    }
}
