mod common;

use common::{git, key};
use fresh_core::apply::{Journal, Outcome, apply, undo_last};
use fresh_core::{Action, Finding, ReviewOptions, Rule, Safety, ScanOptions, review, scan};

/// Moves a temporary worktree through the real Trash and back.
#[test]
fn apply_rechecks_each_finding_and_undo_restores_a_working_checkout() {
    let fx = common::fixture();
    let scan = scan(&fx.root, &ScanOptions::default()).unwrap();
    let opts = ReviewOptions {
        home: fx.root.clone(),
        now: fresh_core::now() + 3 * 86_400,
        min_bytes: 0,
        ..ReviewOptions::default()
    };
    let mut plan: Vec<Finding> =
        review(&scan, &opts).into_iter().filter(|f| matches!(key(f).2.as_str(), "wt-merged" | "feat-merged")).collect();
    assert_eq!(plan.len(), 2);
    // A branch that moved since the review must be left alone.
    plan.push(Finding::new(
        Rule::Branch,
        Safety::Reversible,
        fx.repo.clone(),
        0,
        "feat-open, reviewed at an older commit".into(),
        Action::DeleteBranch { repo: fx.repo.clone(), branch: "feat-open".into(), sha: "0".repeat(40) },
    ));
    let merged_sha = git(&fx.repo, &["rev-parse", "feat-merged"]);
    let open_sha = git(&fx.repo, &["rev-parse", "feat-open"]);
    let worktree = fx.root.join("wt-merged");
    let journal = Journal::at(fx.root.join("journal.jsonl"));

    let results = apply(plan, &journal, |_| {}).unwrap();
    let outcome = |what: &str| &results.iter().find(|a| key(&a.finding).2 == what).unwrap().outcome;
    assert!(matches!(outcome("wt-merged"), Outcome::Applied), "{:?}", outcome("wt-merged"));
    assert!(matches!(outcome("feat-merged"), Outcome::Applied), "{:?}", outcome("feat-merged"));
    assert!(matches!(outcome("feat-open"), Outcome::Skipped { .. }), "{:?}", outcome("feat-open"));
    assert!(!worktree.exists());
    assert!(!git(&fx.repo, &["worktree", "list"]).contains("wt-merged"));
    assert_eq!(git(&fx.repo, &["branch", "--list", "feat-merged"]), "");
    assert_eq!(git(&fx.repo, &["rev-parse", "feat-open"]), open_sha);

    let undone = undo_last(&journal).unwrap().expect("a batch to undo");
    assert!(undone.restored.iter().all(|r| r.error.is_none()), "{:?}", undone.restored);
    assert!(git(&fx.repo, &["worktree", "list"]).contains("wt-merged"));
    assert_eq!(git(&worktree, &["status", "--porcelain"]), "", "restored worktree is a clean checkout");
    assert_eq!(git(&fx.repo, &["rev-parse", "feat-merged"]), merged_sha);
    assert!(undo_last(&journal).unwrap().is_none());
}
