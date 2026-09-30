//! Fresh core: scan a disk, find what can go, and apply reviewed plans safely.
//!
//! The flow is `scan` → `review` → (a person edits the plan) → `apply::apply`, with
//! `apply::undo_last` as the way back.

pub mod apply;
#[cfg(feature = "ffi")]
mod ffi;
pub mod finding;
mod git;
mod rules;
pub mod scan;
mod sys;
pub mod trash;
pub mod treemap;

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub use finding::{Action, Finding, Plan, Rule, Safety};
pub use scan::{Scan, ScanOptions, scan};

#[cfg(feature = "ffi")]
uniffi::setup_scaffolding!();

pub struct ReviewOptions {
    pub home: PathBuf,
    /// Unix seconds that ages are measured from.
    pub now: i64,
    /// Run `git fetch --prune` in each repo first, so deleted upstream branches show up.
    pub fetch: bool,
    /// Installers in Downloads older than this many days are suggested.
    pub installer_days: u32,
    /// Files at least this large and this many days untouched are suggested.
    pub large_file: u64,
    pub large_file_days: u32,
    /// Findings smaller than this are left out; git findings are kept regardless.
    pub min_bytes: u64,
    /// Build output is only suggested once its project has been idle this many days.
    pub build_output_idle_days: u32,
}

impl Default for ReviewOptions {
    fn default() -> Self {
        Self {
            home: home(),
            now: now(),
            fetch: false,
            installer_days: 30,
            large_file: 1 << 30,
            large_file_days: 180,
            min_bytes: 50 << 20,
            build_output_idle_days: 14,
        }
    }
}

/// Everything in the scan worth a look, grouped by rule and largest first.
pub fn review(scan: &Scan, opts: &ReviewOptions) -> Vec<Finding> {
    let mut claims = rules::Claims::new(scan);
    let mut findings = rules::review(scan, opts, &mut claims);
    findings.extend(git::review(scan, &claims, opts.now, opts.fetch));
    findings.retain(|f| {
        (f.rule.is_git() || f.bytes >= opts.min_bytes)
            && (f.rule != Rule::BuildOutput || f.idle_days.is_none_or(|d| d >= opts.build_output_idle_days))
    });
    findings.sort_by(|a, b| a.rule.cmp(&b.rule).then(b.bytes.cmp(&a.bytes)).then_with(|| a.detail.cmp(&b.detail)));
    findings
}

/// The user's home folder.
pub fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

pub fn now() -> i64 {
    now_millis() / 1000
}

fn now_millis() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// Whole days from `then` to `now`, zero if `then` is in the future.
fn days(now: i64, then: i64) -> u32 {
    ((now - then).max(0) / 86_400) as u32
}
