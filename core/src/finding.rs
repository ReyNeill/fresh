//! What a review finds, and the plan a person approves before anything is touched.

use std::fmt::Write as _;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Which rule produced a finding. Also the grouping shown to people.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    BuildOutput,
    Cache,
    Xcode,
    Model,
    Backup,
    Installer,
    LargeFile,
    Trash,
    Worktree,
    Branch,
    RemoteBranch,
}

impl Rule {
    /// Git findings are about history, not bytes, so size thresholds don't apply to them.
    pub fn is_git(self) -> bool {
        matches!(self, Rule::Worktree | Rule::Branch | Rule::RemoteBranch)
    }

    pub fn title(self) -> &'static str {
        match self {
            Rule::BuildOutput => "Build output",
            Rule::Cache => "Caches",
            Rule::Xcode => "Xcode",
            Rule::Model => "ML models",
            Rule::Backup => "Device backups",
            Rule::Installer => "Old installers",
            Rule::LargeFile => "Large old files",
            Rule::Trash => "Trash",
            Rule::Worktree => "Git worktrees",
            Rule::Branch => "Git branches",
            Rule::RemoteBranch => "Remote branches",
        }
    }
}

/// How much thought a finding needs before it goes. Ordered from least to most.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
#[serde(rename_all = "snake_case")]
pub enum Safety {
    /// Rebuilt or re-downloaded automatically when needed.
    Regenerable,
    /// Nothing is lost, and `fresh undo` can put it back.
    Reversible,
    /// May hold something you want; look before it goes.
    Review,
    /// Changes a shared remote, so it affects other people.
    Remote,
}

/// What applying a finding does. Each variant is checked again right before it runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Move to the Trash.
    Trash { path: PathBuf },
    /// Move a clean linked worktree to the Trash, keeping git's record of it inside so undo
    /// restores a working checkout.
    RemoveWorktree { repo: PathBuf, path: PathBuf },
    /// `git worktree prune`: forget worktrees whose folders are already gone.
    PruneWorktrees { repo: PathBuf },
    /// Delete a local branch, only if it still points at `sha`.
    DeleteBranch { repo: PathBuf, branch: String, sha: String },
    /// Delete a branch on a remote, only if it still points at `sha` there.
    DeleteRemoteBranch { repo: PathBuf, remote: String, branch: String, sha: String },
    /// Run a tool's own cleanup command.
    Run { cwd: PathBuf, argv: Vec<String> },
    /// Nothing to apply; shown for information.
    Nothing,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Finding {
    /// Short stable handle for choosing findings on the command line.
    pub id: String,
    pub rule: Rule,
    pub safety: Safety,
    pub path: PathBuf,
    /// Bytes applying this frees, as far as the scan can tell.
    pub bytes: u64,
    /// Days since anything in the relevant project or folder changed.
    pub idle_days: Option<u32>,
    pub detail: String,
    pub action: Action,
}

impl Finding {
    pub fn new(rule: Rule, safety: Safety, path: PathBuf, bytes: u64, detail: String, action: Action) -> Self {
        Self {
            id: short_hash(&format!("{rule:?}{path:?}{action:?}")),
            rule,
            safety,
            path,
            bytes,
            idle_days: None,
            detail,
            action,
        }
    }

    pub fn idle(mut self, days: Option<u32>) -> Self {
        self.idle_days = days;
        self
    }
}

/// A reviewed set of findings, saved to disk so people can edit it before applying.
#[derive(Debug, Serialize, Deserialize)]
pub struct Plan {
    pub root: PathBuf,
    pub created: i64,
    pub findings: Vec<Finding>,
}

/// 8 hex characters of FNV-1a: enough to tell a plan's findings apart.
fn short_hash(s: &str) -> String {
    let hash = s.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3));
    let mut out = String::with_capacity(8);
    write!(out, "{:08x}", hash >> 32).unwrap();
    out
}
