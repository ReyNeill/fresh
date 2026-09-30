//! Alone in its own test binary: it points HOME at a temporary folder for the whole process.

use std::fs;

use fresh_core::apply::{Journal, Outcome, apply};
use fresh_core::{Action, Finding, Rule, Safety};

#[test]
fn hand_edited_plans_cannot_trash_home_or_its_standard_folders() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().canonicalize().unwrap();
    unsafe { std::env::set_var("HOME", &home) };
    let documents = home.join("Documents");
    fs::create_dir(&documents).unwrap();

    let plan = [home.clone(), documents.clone()]
        .map(|path| {
            Finding::new(Rule::LargeFile, Safety::Review, path.clone(), 0, "edited in".into(), Action::Trash { path })
        })
        .to_vec();
    let results = apply(plan, &Journal::at(home.join("journal.jsonl")), |_| {}).unwrap();

    for a in &results {
        assert!(matches!(&a.outcome, Outcome::Failed { error } if error.starts_with("refusing")), "{a:?}");
    }
    assert!(documents.exists());
}
