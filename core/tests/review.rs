mod common;

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, SystemTime};

use fresh_core::{ReviewOptions, Rule, Safety, ScanOptions, freed, review, scan};

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
    let findings =
        review(&scan, &ReviewOptions { home: root.clone(), min_bytes: 0, ..ReviewOptions::default() }).findings;

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
    let findings =
        review(&scan, &ReviewOptions { home: root.clone(), min_bytes: 0, ..ReviewOptions::default() }).findings;

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
    // Something is working inside `wt-busy`, so it's in use however finished it looks.
    let mut busy = Command::new("sleep").arg("60").current_dir(fx.root.join("wt-busy")).spawn().unwrap();
    let scan = scan(&fx.root, &ScanOptions::default()).unwrap();
    // Three days on: past the grace period for finished worktrees, short of stale.
    let opts = ReviewOptions {
        home: fx.root.clone(),
        now: fresh_core::now() + 3 * 86_400,
        fetch: true,
        min_bytes: 0,
        ..ReviewOptions::default()
    };

    let mut found: Vec<_> = review(&scan, &opts).findings.iter().map(common::key).collect();
    busy.kill().unwrap();
    busy.wait().unwrap();
    found.sort();

    let expected = [
        (Rule::Worktree, Safety::Reversible, "wt-merged"),
        (Rule::Worktree, Safety::Review, "wt-detached"),
        (Rule::Branch, Safety::Reversible, "feat-merged"),
        (Rule::Branch, Safety::Review, "feat-gone"),
        (Rule::RemoteBranch, Safety::Remote, "shipped"),
    ]
    .map(|(rule, safety, what)| (rule, safety, what.to_owned()));
    assert_eq!(found, expected);
}

fn write_bytes(path: &Path, len: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let data: Vec<u8> = (0..len as u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8).collect();
    fs::write(path, data).unwrap();
}

/// An APFS clone: a new file sharing every block with `from`.
fn clone(from: &Path, to: &Path) {
    fs::create_dir_all(to.parent().unwrap()).unwrap();
    assert!(Command::new("cp").arg("-c").arg(from).arg(to).status().unwrap().success());
}

#[test]
fn shared_data_frees_only_when_every_copy_goes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let blocks = |rel: &str| fs::symlink_metadata(root.join(rel)).unwrap().blocks() * 512;
    // Bun clones its cache into node_modules: neither frees the data alone, both together do.
    write(&root.join("web/package.json"));
    write_bytes(&root.join("web/node_modules/dep.bin"), 3 << 20);
    clone(&root.join("web/node_modules/dep.bin"), &root.join(".bun/install/cache/dep.bin"));
    // Cargo clones inside one target folder: the folder alone frees the data once.
    write(&root.join("api/Cargo.toml"));
    write_bytes(&root.join("api/target/a.bin"), 2 << 20);
    clone(&root.join("api/target/a.bin"), &root.join("api/target/b.bin"));
    // A clone and a hard link kept outside every finding: nothing is freed.
    write(&root.join("site/package.json"));
    write_bytes(&root.join("keep/lib.bin"), 1 << 20);
    clone(&root.join("keep/lib.bin"), &root.join("site/node_modules/lib.bin"));
    write_bytes(&root.join("keep/linked.bin"), 1 << 20);
    fs::hard_link(root.join("keep/linked.bin"), root.join("site/node_modules/linked.bin")).unwrap();

    let scan = scan(&root, &ScanOptions::default()).unwrap();
    let opts =
        ReviewOptions { home: root.clone(), min_bytes: 0, build_output_idle_days: 0, ..ReviewOptions::default() };
    let review = review(&scan, &opts);
    let finding = |rel: &str| {
        review
            .findings
            .iter()
            .find(|f| f.path == root.join(rel))
            .unwrap_or_else(|| panic!("no finding for {rel}"))
            .clone()
    };
    let (cache, web, target, site) = (
        finding(".bun/install/cache"),
        finding("web/node_modules"),
        finding("api/target"),
        finding("site/node_modules"),
    );

    assert_eq!((cache.bytes, web.bytes), (0, 0), "each copy alone frees nothing");
    assert_eq!(freed(&[cache.clone(), web.clone()], &review.joints), blocks("web/node_modules/dep.bin"));
    assert_eq!(target.bytes, blocks("api/target/a.bin"), "clones inside one finding free the data once");
    assert_eq!(target.size, blocks("api/target/a.bin") + blocks("api/target/b.bin"));
    assert_eq!(site.bytes, 0, "copies outside every finding keep the data");
}
