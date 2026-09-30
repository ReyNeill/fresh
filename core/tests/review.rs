mod common;

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use fresh_core::{ReviewOptions, Rule, Safety, ScanOptions, review, scan};

fn write(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "x").unwrap();
}

#[test]
fn build_outputs_come_only_from_projects_and_idle_ignores_the_output() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    // A project whose files were last edited 40 days ago. Creating its dependencies just now
    // bumped the project folder's own timestamp, as moving them to the Trash and back would.
    write(&root.join("app/package.json"));
    write(&root.join("app/node_modules/dep/index.js"));
    let forty_days_ago = SystemTime::now() - Duration::from_secs(40 * 86_400 + 60);
    fs::File::open(root.join("app/package.json")).unwrap().set_modified(forty_days_ago).unwrap();
    // Same names, but load-bearing: no project, a tool's install in a home dotfolder, and a
    // tool's own venv (uv tags every venv as a cache).
    write(&root.join("loose/node_modules/dep/index.js"));
    write(&root.join(".opencode/package.json"));
    write(&root.join(".opencode/node_modules/dep/index.js"));
    write(&root.join("tools/agent/kernel-venv/CACHEDIR.TAG"));

    let scan = scan(&root, &ScanOptions::default()).unwrap();
    let findings = review(&scan, &ReviewOptions { home: root.clone(), min_bytes: 0, ..ReviewOptions::default() });

    let outputs: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == Rule::BuildOutput)
        .map(|f| (f.path.strip_prefix(&root).unwrap().to_path_buf(), f.safety, f.idle_days))
        .collect();
    assert_eq!(outputs, [(Path::new("app/node_modules").to_path_buf(), Safety::Regenerable, Some(40))]);
}

#[test]
fn caches_of_running_programs_need_review() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    // This test's own executable is named after the test file, so a `review-…` cache is in use.
    let this_program = std::env::current_exe().unwrap();
    let running = this_program.file_name().unwrap().to_str().unwrap().split('-').next().unwrap();
    write(&root.join(format!("Library/Caches/{running}-cache/blob")));
    write(&root.join("Library/Caches/qwzxvbundle-cache/blob"));

    let scan = scan(&root, &ScanOptions::default()).unwrap();
    let findings = review(&scan, &ReviewOptions { home: root.clone(), min_bytes: 0, ..ReviewOptions::default() });

    let caches: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == Rule::Cache)
        .map(|f| (f.path.file_name().unwrap().to_str().unwrap().to_owned(), f.safety))
        .collect();
    assert!(caches.contains(&(format!("{running}-cache"), Safety::Review)), "{caches:?}");
    assert!(caches.contains(&("qwzxvbundle-cache".to_owned(), Safety::Regenerable)), "{caches:?}");
}

#[test]
fn git_findings_count_work_merged_into_any_long_lived_branch() {
    let fx = common::fixture();
    let scan = scan(&fx.root, &ScanOptions::default()).unwrap();
    // Three days on: past the grace period for finished worktrees, short of stale.
    let opts = ReviewOptions {
        home: fx.root.clone(),
        now: fresh_core::now() + 3 * 86_400,
        fetch: true,
        min_bytes: 0,
        ..ReviewOptions::default()
    };

    let mut found: Vec<_> = review(&scan, &opts).iter().map(common::key).collect();
    found.sort();

    let expected = [
        (Rule::Worktree, Safety::Reversible, "wt-merged"),
        (Rule::Branch, Safety::Reversible, "feat-merged"),
        (Rule::Branch, Safety::Review, "feat-gone"),
        (Rule::RemoteBranch, Safety::Remote, "shipped"),
    ]
    .map(|(rule, safety, what)| (rule, safety, what.to_owned()));
    assert_eq!(found, expected);
}
