//! Git fixture shared by the review and apply tests.

// Compiled into each test binary, and each uses a different part.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use fresh_core::{Action, Finding, Rule, Safety};
use tempfile::TempDir;

/// Runs git with the user's global config ignored, so hooks or signing can't interfere.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

pub struct Fixture {
    _dir: TempDir,
    pub root: PathBuf,
    pub repo: PathBuf,
}

/// A repo whose remote's default branch is `prod` while work merges into `dev`, with:
/// - worktree `wt-merged` on `feat-merged`: merged into dev, clean
/// - worktree `wt-dirty` on `feat-dirty`: merged into dev, with an uncommitted edit
/// - `feat-open`: unmerged, no worktree
/// - `feat-gone`: pushed, then deleted on the remote
/// - `origin/shipped`: merged into dev, authored by the repo's user, no local branch
pub fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let repo = root.join("repo");
    git(&root, &["init", "-q", "--bare", "-b", "prod", "origin.git"]);
    git(&root, &["init", "-q", "-b", "prod", "repo"]);
    git(&repo, &["config", "user.email", "me@example.com"]);
    git(&repo, &["config", "user.name", "Me"]);
    commit(&repo, "README");
    git(&repo, &["remote", "add", "origin", root.join("origin.git").to_str().unwrap()]);
    git(&repo, &["push", "-q", "-u", "origin", "prod"]);
    git(&repo, &["switch", "-q", "-c", "dev"]);
    git(&repo, &["push", "-q", "-u", "origin", "dev"]);
    git(&repo, &["remote", "set-head", "origin", "prod"]);

    for branch in ["feat-merged", "shipped"] {
        git(&repo, &["switch", "-q", "-c", branch, "dev"]);
        commit(&repo, branch);
        git(&repo, &["switch", "-q", "dev"]);
        git(&repo, &["merge", "-q", "--no-ff", "-m", &format!("merge {branch}"), branch]);
    }
    git(&repo, &["push", "-q", "origin", "dev", "shipped"]);
    git(&repo, &["branch", "-q", "-D", "shipped"]);
    git(&repo, &["branch", "-q", "feat-dirty", "dev"]);
    git(&repo, &["switch", "-q", "-c", "feat-open", "dev"]);
    commit(&repo, "open");
    git(&repo, &["switch", "-q", "-c", "feat-gone", "dev"]);
    commit(&repo, "gone");
    git(&repo, &["push", "-q", "-u", "origin", "feat-gone"]);
    git(&repo, &["push", "-q", "origin", "--delete", "feat-gone"]);
    git(&repo, &["switch", "-q", "dev"]);
    git(&repo, &["worktree", "add", "-q", "../wt-merged", "feat-merged"]);
    git(&repo, &["worktree", "add", "-q", "../wt-dirty", "feat-dirty"]);
    fs::write(root.join("wt-dirty/README"), "edited").unwrap();
    Fixture { _dir: dir, root, repo }
}

fn commit(repo: &Path, file: &str) {
    fs::write(repo.join(file), file).unwrap();
    git(repo, &["add", file]);
    git(repo, &["commit", "-q", "-m", file]);
}

/// A finding reduced to what it is about: the worktree folder or branch name.
pub fn key(f: &Finding) -> (Rule, Safety, String) {
    let what = match &f.action {
        Action::RemoveWorktree { path, .. } => path.file_name().unwrap().to_string_lossy().into_owned(),
        Action::DeleteBranch { branch, .. } | Action::DeleteRemoteBranch { branch, .. } => branch.clone(),
        other => format!("{other:?}"),
    };
    (f.rule, f.safety, what)
}
