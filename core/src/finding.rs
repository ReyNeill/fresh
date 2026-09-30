//! What a review finds, and the plan a person approves before anything is touched.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

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

impl Action {
    /// Whether applying this removes the files at the finding's path.
    pub fn removes_files(&self) -> bool {
        matches!(self, Action::Trash { .. } | Action::RemoveWorktree { .. })
    }

    /// Whether applying this frees disk space: by removing files, or through a tool's own
    /// cleanup (deleting a simulator runtime, say).
    pub fn frees_space(&self) -> bool {
        self.removes_files() || matches!(self, Action::Run { .. })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Finding {
    /// Short stable handle for choosing findings on the command line.
    pub id: String,
    pub rule: Rule,
    pub safety: Safety,
    pub path: PathBuf,
    /// Bytes applying this alone frees. Less than `size` when it shares data with copies
    /// (APFS clones, hard links) that stay behind; see `Joint` for copies that go together.
    pub bytes: u64,
    /// Space it takes up on disk.
    #[serde(default)]
    pub size: u64,
    /// Days since anything in the relevant project or folder changed.
    pub idle_days: Option<u32>,
    pub detail: String,
    pub action: Action,
    /// A name to show instead of the path's last component, for things whose path says
    /// little ("iOS 26.5 Simulator").
    #[serde(default)]
    pub label: Option<String>,
}

impl Finding {
    pub fn new(rule: Rule, safety: Safety, path: PathBuf, bytes: u64, detail: String, action: Action) -> Self {
        Self {
            id: short_hash(&format!("{rule:?}{path:?}{action:?}")),
            rule,
            safety,
            path,
            bytes,
            size: bytes,
            idle_days: None,
            detail,
            action,
            label: None,
        }
    }

    pub fn labeled(mut self, label: String) -> Self {
        self.label = Some(label);
        self
    }

    pub fn idle(mut self, days: Option<u32>) -> Self {
        self.idle_days = days;
        self
    }
}

/// Space that only frees when every one of `findings` is applied: data their copies share,
/// with no copy anywhere else. The Bun cache and the `node_modules` it cloned into, say.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Joint {
    /// Ids of the findings that must all be applied.
    pub findings: Vec<String>,
    pub bytes: u64,
}

/// A reviewed set of findings, saved to disk so people can edit it before applying.
#[derive(Debug, Serialize, Deserialize)]
pub struct Plan {
    pub root: PathBuf,
    pub created: i64,
    pub findings: Vec<Finding>,
    #[serde(default)]
    pub joints: Vec<Joint>,
}

/// Space applying `findings` together frees: each finding's own bytes with nested ones
/// counted once, plus every joint saving whose findings are all included.
pub fn freed(findings: &[Finding], joints: &[Joint]) -> u64 {
    let mut freeing: Vec<&Finding> = findings.iter().filter(|f| f.action.frees_space()).collect();
    freeing.sort_by(|a, b| a.path.cmp(&b.path));
    let mut total = 0;
    let mut outer: Option<&Path> = None;
    for finding in freeing {
        // Paths sort by component, so anything inside `outer` follows it directly.
        if outer.is_some_and(|o| finding.path.starts_with(o)) {
            continue;
        }
        total += finding.bytes;
        outer = Some(&finding.path);
    }
    let ids: HashSet<&str> = findings.iter().map(|f| f.id.as_str()).collect();
    total
        + joints.iter().filter(|j| j.findings.iter().all(|id| ids.contains(id.as_str()))).map(|j| j.bytes).sum::<u64>()
}

/// 8 hex characters of FNV-1a: enough to tell a plan's findings apart.
fn short_hash(s: &str) -> String {
    let hash = s.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3));
    let mut out = String::with_capacity(8);
    write!(out, "{:08x}", hash >> 32).unwrap();
    out
}
