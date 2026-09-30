//! Applies a reviewed plan: re-checks each finding against the disk as it is now, acts, and
//! journals how to undo it.

use std::collections::HashSet;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::finding::{Action, Finding};
use crate::git::git;
use crate::trash::trash;

/// One step that reverses part of an applied finding.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Undo {
    MoveBack { from: PathBuf, to: PathBuf },
    Git { repo: PathBuf, args: Vec<String> },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    Applied { batch: i64, time: i64, finding: Finding, undo: Vec<Undo> },
    Failed { batch: i64, time: i64, finding: Finding, error: String },
    Undone { batch: i64, time: i64 },
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum Outcome {
    Applied,
    /// The disk changed since the review, so the finding no longer holds.
    Skipped {
        reason: String,
    },
    Failed {
        error: String,
    },
}

/// A finding and what happened when it was applied.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Applied {
    pub finding: Finding,
    pub outcome: Outcome,
}

/// Append-only JSON Lines record of everything applied.
pub struct Journal {
    path: PathBuf,
}

impl Journal {
    /// `~/Library/Application Support/fresh/journal.jsonl`, shared by the CLI and the app.
    pub fn default_location() -> Self {
        Self::at(crate::home().join("Library/Application Support/fresh/journal.jsonl"))
    }

    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    fn append(&self, entry: &Entry) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut file = fs::OpenOptions::new().create(true).append(true).open(&self.path)?;
        writeln!(file, "{}", serde_json::to_string(entry)?)
    }

    pub fn entries(&self) -> io::Result<Vec<Entry>> {
        let file = match fs::File::open(&self.path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        io::BufReader::new(file).lines().map(|line| serde_json::from_str(&line?).map_err(io::Error::other)).collect()
    }
}

/// Applies findings, worktrees before branches (git won't delete a checked-out branch) and
/// quick git edits before slow moves. `progress` sees each result as it happens.
pub fn apply(
    mut findings: Vec<Finding>,
    journal: &Journal,
    mut progress: impl FnMut(&Applied),
) -> io::Result<Vec<Applied>> {
    findings.retain(|f| f.action != Action::Nothing);
    findings.sort_by_key(|f| match f.action {
        Action::RemoveWorktree { .. } => 0,
        Action::PruneWorktrees { .. } => 1,
        Action::DeleteBranch { .. } => 2,
        Action::DeleteRemoteBranch { .. } => 3,
        Action::Trash { .. } => 4,
        Action::Run { .. } | Action::Nothing => 5,
    });
    let batch = crate::now_millis();
    let mut results = Vec::with_capacity(findings.len());
    for finding in findings {
        let time = crate::now();
        let outcome = match act(&finding.action) {
            Ok(undo) => {
                journal.append(&Entry::Applied { batch, time, finding: finding.clone(), undo })?;
                Outcome::Applied
            }
            Err(Problem::Skip(reason)) => Outcome::Skipped { reason },
            Err(Problem::Fail(error)) => {
                journal.append(&Entry::Failed { batch, time, finding: finding.clone(), error: error.clone() })?;
                Outcome::Failed { error }
            }
        };
        let applied = Applied { finding, outcome };
        progress(&applied);
        results.push(applied);
    }
    Ok(results)
}

enum Problem {
    Skip(String),
    Fail(String),
}

use Problem::{Fail, Skip};

fn act(action: &Action) -> Result<Vec<Undo>, Problem> {
    match action {
        Action::Trash { path } => {
            guard(path)?;
            if fs::symlink_metadata(path).is_err() {
                return Err(Skip("already gone".into()));
            }
            let landed = trash(path).map_err(Fail)?;
            Ok(vec![Undo::MoveBack { from: landed, to: path.clone() }])
        }
        Action::RemoveWorktree { repo, path } => remove_worktree(repo, path),
        Action::PruneWorktrees { repo } => {
            git(repo, &["worktree", "prune"]).map_err(Fail)?;
            Ok(Vec::new())
        }
        Action::DeleteBranch { repo, branch, sha } => {
            let current = git(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")])
                .map_err(|_| Skip("branch no longer exists".into()))?;
            if current.trim() != sha {
                return Err(Skip("branch moved since the review".into()));
            }
            git(repo, &["branch", "-D", branch]).map_err(Fail)?;
            Ok(vec![Undo::Git { repo: repo.clone(), args: vec!["branch".into(), branch.clone(), sha.clone()] }])
        }
        Action::DeleteRemoteBranch { repo, remote, branch, sha } => {
            // The lease makes the push fail if someone moved the branch since the review.
            let lease = format!("--force-with-lease=refs/heads/{branch}:{sha}");
            git(repo, &["push", &lease, remote, &format!(":refs/heads/{branch}")]).map_err(|e| {
                if e.contains("remote ref does not exist") {
                    Skip("already deleted on the remote".into())
                } else {
                    Fail(e)
                }
            })?;
            Ok(vec![Undo::Git {
                repo: repo.clone(),
                args: vec!["push".into(), remote.clone(), format!("{sha}:refs/heads/{branch}")],
            }])
        }
        Action::Run { cwd, argv } => {
            let (program, args) = argv.split_first().ok_or_else(|| Fail("empty command".into()))?;
            let out = Command::new(program)
                .args(args)
                .current_dir(cwd)
                .stdin(Stdio::null())
                .output()
                .map_err(|e| Fail(format!("{program}: {e}")))?;
            if !out.status.success() {
                return Err(Fail(String::from_utf8_lossy(&out.stderr).trim().to_owned()));
            }
            Ok(Vec::new())
        }
        Action::Nothing => Err(Skip("nothing to apply".into())),
    }
}

/// Moves a clean linked worktree to the Trash. Git's record of the worktree (its HEAD, index
/// and reflog under `.git/worktrees/<name>`) moves inside the folder first: git then treats
/// the worktree as removed, and undo can put both back for a working checkout.
fn remove_worktree(repo: &Path, path: &Path) -> Result<Vec<Undo>, Problem> {
    guard(path)?;
    if !path.exists() {
        return Err(Skip("already gone".into()));
    }
    let listed = git(repo, &["worktree", "list", "--porcelain"]).map_err(Fail)?;
    if !listed.lines().any(|l| l.strip_prefix("worktree ") == path.to_str()) {
        return Err(Skip(format!("no longer a worktree of {}", repo.display())));
    }
    match git(path, &["status", "--porcelain"]) {
        Ok(status) if status.is_empty() => {}
        Ok(_) => return Err(Skip("has uncommitted changes now".into())),
        Err(e) => return Err(Fail(e)),
    }
    let pointer = fs::read_to_string(path.join(".git")).map_err(|e| Fail(format!("reading .git: {e}")))?;
    let admin = pointer
        .strip_prefix("gitdir: ")
        .map(|p| path.join(p.trim()))
        .ok_or_else(|| Fail("unexpected .git file".into()))?;
    let stash = path.join(".fresh-worktree");
    if stash.exists() {
        return Err(Fail(format!("{} already exists", stash.display())));
    }
    fs::rename(&admin, &stash).map_err(|e| Fail(format!("moving git's worktree record: {e}")))?;
    match trash(path) {
        Ok(landed) => {
            Ok(vec![Undo::MoveBack { from: landed, to: path.to_owned() }, Undo::MoveBack { from: stash, to: admin }])
        }
        Err(e) => {
            let _ = fs::rename(&stash, &admin);
            Err(Fail(e))
        }
    }
}

/// Refuses paths no finding should ever touch, whatever a hand-edited plan says.
fn guard(path: &Path) -> Result<(), Problem> {
    const STANDARD: &[&str] =
        &["Applications", "Desktop", "Documents", "Downloads", "Library", "Movies", "Music", "Pictures", "Public"];
    let home = crate::home();
    let standard = path.parent() == Some(&home) && path.file_name().is_some_and(|n| STANDARD.iter().any(|s| n == *s));
    if !path.is_absolute() || home.starts_with(path) || standard || path.components().count() < 3 {
        return Err(Fail(format!("refusing to touch {}", path.display())));
    }
    Ok(())
}

/// A reversed batch and how putting back each of its findings went.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Undone {
    pub batch: i64,
    pub restored: Vec<Restored>,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Restored {
    pub finding: Finding,
    /// Why it couldn't be put back, if it couldn't.
    pub error: Option<String>,
}

/// Reverses the most recent batch that hasn't been undone, newest action first.
/// `None` when there is nothing to undo.
pub fn undo_last(journal: &Journal) -> io::Result<Option<Undone>> {
    let entries = journal.entries()?;
    let undone: HashSet<i64> = entries
        .iter()
        .filter_map(|e| match e {
            Entry::Undone { batch, .. } => Some(*batch),
            _ => None,
        })
        .collect();
    let Some(batch) = entries.iter().rev().find_map(|e| match e {
        Entry::Applied { batch, undo, .. } if !undo.is_empty() && !undone.contains(batch) => Some(*batch),
        _ => None,
    }) else {
        return Ok(None);
    };

    let mut restored = Vec::new();
    for entry in entries.into_iter().rev() {
        let Entry::Applied { batch: b, finding, undo, .. } = entry else { continue };
        if b != batch {
            continue;
        }
        let result = undo.iter().try_for_each(|step| match step {
            Undo::MoveBack { from, to } => {
                if to.exists() {
                    return Err(format!("something is already at {}", to.display()));
                }
                fs::rename(from, to).map_err(|e| format!("moving {} back: {e}", from.display()))
            }
            Undo::Git { repo, args } => {
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                git(repo, &args).map(drop)
            }
        });
        restored.push(Restored { finding, error: result.err() });
    }
    journal.append(&Entry::Undone { batch, time: crate::now() })?;
    Ok(Some(Undone { batch, restored }))
}
