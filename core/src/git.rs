//! Git hygiene: finished worktrees, merged or orphaned branches, and your merged branches
//! still sitting on the remote.
//!
//! Reads go through the `git` binary so user config is respected, with optional locks off so
//! a review never rewrites anyone's index.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rayon::prelude::*;

use crate::finding::{Action, Finding, Rule, Safety};
use crate::rules::{Claims, is_tool_dir, walk};
use crate::scan::{Markers, NodeKind, Scan, ScanOptions};

/// Long-lived branch names never suggested for deletion.
const PROTECTED: &[&str] =
    &["main", "master", "dev", "develop", "development", "staging", "prod", "production", "release", "trunk"];
/// Clean worktrees on unfinished branches, and in-sync local copies, are suggested after this.
const STALE_DAYS: u32 = 30;
/// Finished worktrees still wait this long, so one created a minute ago is left alone.
const GRACE_DAYS: u32 = 2;

/// Runs git in `dir`; stdout on success, stderr (or the spawn error) on failure.
pub(crate) fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

pub(crate) fn review(scan: &Scan, claims: &Claims, now: i64, fetch: bool) -> Vec<Finding> {
    let repos = discover(scan, claims);
    crate::scan::pool().install(|| {
        repos
            .par_iter()
            .flat_map_iter(|repo| {
                if fetch {
                    let _ = git(repo, &["fetch", "--all", "--prune", "--quiet"]);
                }
                analyze(scan, repo, now, fetch)
            })
            .collect()
    })
}

/// Working repositories in the scan: folders with a `.git` directory, outside caches and
/// tool installs. Linked worktrees have a `.git` file and are reached through their repo.
fn discover(scan: &Scan, claims: &Claims) -> Vec<PathBuf> {
    let mut repos = Vec::new();
    walk(scan, 0, claims, |id, node| {
        if node.kind == NodeKind::File || node.unread || is_tool_dir(&node.name) || &*node.name == "node_modules" {
            return false;
        }
        if node.markers.intersects(Markers::GIT_DIR) {
            repos.push(scan.path(id));
        }
        true
    });
    repos
}

#[derive(Debug, Default)]
struct Worktree {
    path: PathBuf,
    head: String,
    branch: Option<String>,
    locked: bool,
    prunable: bool,
}

/// Parses `git worktree list --porcelain`. The first entry is the main checkout.
fn parse_worktrees(porcelain: &str) -> Vec<Worktree> {
    porcelain
        .split("\n\n")
        .filter(|block| !block.trim().is_empty())
        .map(|block| {
            let mut wt = Worktree::default();
            for line in block.lines() {
                let (key, value) = line.split_once(' ').unwrap_or((line, ""));
                match key {
                    "worktree" => wt.path = PathBuf::from(value),
                    "HEAD" => wt.head = value.to_owned(),
                    "branch" => wt.branch = value.strip_prefix("refs/heads/").map(str::to_owned),
                    "locked" => wt.locked = true,
                    "prunable" => wt.prunable = true,
                    _ => {}
                }
            }
            wt
        })
        .collect()
}

struct Branch {
    name: String,
    sha: String,
    upstream: String,
    /// `[gone]`, `[ahead 1]`, ... or empty when in sync.
    track: String,
    date: i64,
}

fn branches(repo: &Path) -> Vec<Branch> {
    let format =
        "--format=%(refname:short)%00%(objectname)%00%(upstream:short)%00%(upstream:track)%00%(committerdate:unix)";
    git(repo, &["for-each-ref", format, "refs/heads"])
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let mut f = line.split('\0');
            Some(Branch {
                name: f.next()?.to_owned(),
                sha: f.next()?.to_owned(),
                upstream: f.next()?.to_owned(),
                track: f.next()?.to_owned(),
                date: f.next()?.parse().ok()?,
            })
        })
        .collect()
}

/// Branches finished work lands in: the remote's default branch plus every protected name
/// it has, since work often merges into dev long before it reaches prod. Repos without a
/// remote fall back to local main or master.
fn bases(repo: &Path) -> Vec<String> {
    let mut bases: Vec<String> = git(repo, &["symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD"])
        .ok()
        .map(|head| head.trim().to_owned())
        .into_iter()
        .collect();
    let remote_refs =
        git(repo, &["for-each-ref", "--format=%(refname:short)", "refs/remotes/origin"]).unwrap_or_default();
    for name in PROTECTED {
        let candidate = format!("origin/{name}");
        if !bases.contains(&candidate) && remote_refs.lines().any(|r| r == candidate) {
            bases.push(candidate);
        }
    }
    if bases.is_empty()
        && let Some(local) = ["main", "master"]
            .into_iter()
            .find(|b| git(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{b}")]).is_ok())
    {
        bases.push(local.to_owned());
    }
    bases
}

/// Refs under `refs` merged into any of `bases`, each mapped to the first base that has it.
fn merged_into(repo: &Path, bases: &[String], refs: &str) -> HashMap<String, String> {
    let mut merged = HashMap::new();
    for base in bases {
        let names =
            git(repo, &["for-each-ref", "--merged", base, "--format=%(refname:short)", refs]).unwrap_or_default();
        for name in names.lines() {
            merged.entry(name.to_owned()).or_insert_with(|| base.clone());
        }
    }
    merged
}

/// Bytes and newest change time of a worktree: from the scan when it covers the worktree,
/// otherwise from a quick scan of just that folder (agents often keep worktrees in temp dirs).
fn measure(scan: &Scan, path: &Path) -> Option<(u64, i64)> {
    if let Some(id) = scan.find(path) {
        return Some((scan.node(id).bytes, scan.node(id).newest));
    }
    let own = crate::scan(path, &ScanOptions { big_file: u64::MAX, ..ScanOptions::default() }).ok()?;
    Some((own.node(0).bytes, own.node(0).newest))
}

/// Why a worktree's work is finished, if it is.
enum Progress {
    Done(String),
    Open,
    /// Detached on commits no branch has: removing it would lose them.
    Orphaned,
}

fn analyze(scan: &Scan, repo: &Path, now: i64, fetched: bool) -> Vec<Finding> {
    let mut out = Vec::new();
    let Ok(list) = git(repo, &["worktree", "list", "--porcelain"]) else { return out };
    let worktrees = parse_worktrees(&list);
    let repo_name = repo.file_name().map_or_else(|| repo.display().to_string(), |n| n.to_string_lossy().into_owned());

    let stale_records = worktrees.iter().skip(1).filter(|w| w.prunable).count();
    if stale_records > 0 {
        out.push(Finding::new(
            Rule::Worktree,
            Safety::Regenerable,
            repo.to_owned(),
            0,
            format!("{stale_records} worktree records whose folders are already gone"),
            Action::PruneWorktrees { repo: repo.to_owned() },
        ));
    }

    let bases = bases(repo);
    if bases.is_empty() {
        return out;
    }
    let is_long_lived = |name: &str| {
        PROTECTED.contains(&name) || bases.iter().any(|b| b.rsplit_once('/').map_or(b.as_str(), |(_, n)| n) == name)
    };
    let merged = merged_into(repo, &bases, "refs/heads");
    let branches = branches(repo);
    let upstream_gone = |name: &str| branches.iter().any(|b| b.name == name && b.track == "[gone]");

    // Branches of worktrees suggested for removal; those branches may then go too.
    let mut leaving: HashSet<&str> = HashSet::new();
    for wt in worktrees.iter().skip(1).filter(|w| !w.prunable && !w.locked) {
        let Some((bytes, newest)) = measure(scan, &wt.path) else { continue };
        let idle = crate::days(now, newest);
        let progress = match &wt.branch {
            Some(b) if merged.contains_key(b) => Progress::Done(format!("{b} is merged into {}", merged[b])),
            Some(b) if upstream_gone(b) => Progress::Done(format!("{b} was deleted upstream")),
            Some(_) => Progress::Open,
            None if contained(repo, &wt.head) => Progress::Done("detached on commits a branch already has".into()),
            None => Progress::Orphaned,
        };
        let clean = git(&wt.path, &["status", "--porcelain"]).is_ok_and(|s| s.is_empty());
        let on = wt.branch.as_deref().unwrap_or("detached HEAD");
        let remove = Action::RemoveWorktree { repo: repo.to_owned(), path: wt.path.clone() };
        let finding = match (progress, clean) {
            (Progress::Done(why), true) if idle >= GRACE_DAYS => Finding::new(
                Rule::Worktree,
                Safety::Reversible,
                wt.path.clone(),
                bytes,
                format!("worktree of {repo_name}; {why}"),
                remove,
            ),
            (Progress::Open, true) if idle >= STALE_DAYS => Finding::new(
                Rule::Worktree,
                Safety::Review,
                wt.path.clone(),
                bytes,
                format!("worktree of {repo_name}; clean, and {on} is unmerged but stays"),
                remove,
            ),
            (_, false) if idle >= STALE_DAYS => Finding::new(
                Rule::Worktree,
                Safety::Review,
                wt.path.clone(),
                bytes,
                format!("worktree of {repo_name}; uncommitted changes on {on}, commit or discard them first"),
                Action::Nothing,
            ),
            _ => continue,
        };
        if matches!(finding.action, Action::RemoveWorktree { .. })
            && let Some(branch) = &wt.branch
        {
            leaving.insert(branch);
        }
        out.push(finding.idle(Some(idle)));
    }

    let checked_out: HashSet<&str> = worktrees.iter().filter_map(|w| w.branch.as_deref()).collect();
    for b in &branches {
        if is_long_lived(&b.name) || (checked_out.contains(b.name.as_str()) && !leaving.contains(b.name.as_str())) {
            continue;
        }
        let idle = crate::days(now, b.date);
        let (safety, detail) = if let Some(base) = merged.get(&b.name) {
            (Safety::Reversible, format!("merged into {base}"))
        } else if b.track == "[gone]" {
            let ahead = bases
                .iter()
                .filter_map(|base| git(repo, &["rev-list", "--count", &format!("{base}..{}", b.name)]).ok())
                .filter_map(|n| n.trim().parse::<u32>().ok())
                .min()
                .unwrap_or(0);
            (
                Safety::Review,
                format!(
                    "{} was deleted, usually after its PR merged; {ahead} commits aren't on {}",
                    b.upstream, bases[0]
                ),
            )
        } else if !b.upstream.is_empty() && b.track.is_empty() && idle >= STALE_DAYS {
            (Safety::Reversible, format!("identical to {}; the local copy is redundant", b.upstream))
        } else {
            continue;
        };
        out.push(
            Finding::new(
                Rule::Branch,
                safety,
                repo.to_owned(),
                0,
                detail,
                Action::DeleteBranch { repo: repo.to_owned(), branch: b.name.clone(), sha: b.sha.clone() },
            )
            .idle(Some(idle)),
        );
    }

    // Remote-tracking refs linger after branches are deleted on the server, so remote
    // suggestions are only trustworthy right after a fetch.
    if fetched {
        out.extend(remote_branches(repo, &bases, &is_long_lived, now));
    }
    out
}

/// Whether some branch or remote branch already contains `sha`.
fn contained(repo: &Path, sha: &str) -> bool {
    git(repo, &["for-each-ref", "--contains", sha, "--count=1", "--format=x", "refs/heads", "refs/remotes"])
        .is_ok_and(|s| !s.trim().is_empty())
}

/// Your branches on origin that are merged into one of the bases.
fn remote_branches(repo: &Path, bases: &[String], is_long_lived: &dyn Fn(&str) -> bool, now: i64) -> Vec<Finding> {
    let Ok(email) = git(repo, &["config", "user.email"]) else { return Vec::new() };
    let email = format!("<{}>", email.trim().to_lowercase());
    let merged = merged_into(repo, bases, "refs/remotes/origin");
    let format = "--format=%(refname:short)%00%(objectname)%00%(authoremail)%00%(committerdate:unix)";
    git(repo, &["for-each-ref", format, "refs/remotes/origin"])
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let mut f = line.split('\0');
            let (name, sha, author, date) = (f.next()?, f.next()?, f.next()?, f.next()?.parse::<i64>().ok()?);
            // Plain `origin` is the remote's HEAD symref.
            let branch = name.strip_prefix("origin/")?;
            let base = merged.get(name)?;
            if is_long_lived(branch) || author.to_lowercase() != email {
                return None;
            }
            Some(
                Finding::new(
                    Rule::RemoteBranch,
                    Safety::Remote,
                    repo.to_owned(),
                    0,
                    format!("yours, and merged into {base}"),
                    Action::DeleteRemoteBranch {
                        repo: repo.to_owned(),
                        remote: "origin".to_owned(),
                        branch: branch.to_owned(),
                        sha: sha.to_owned(),
                    },
                )
                .idle(Some(crate::days(now, date))),
            )
        })
        .collect()
}
