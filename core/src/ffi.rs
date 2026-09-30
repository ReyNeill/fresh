//! The API the Swift app links against, exported through UniFFI.

use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::apply::{self, Applied, Journal, Undone};
use crate::treemap::{Atlas, SpaceMap};
use crate::{Finding, Joint, ReviewOptions, Scan, ScanOptions};

uniffi::custom_type!(PathBuf, String, {
    remote,
    lower: |path| path.to_string_lossy().into_owned(),
    try_lift: |text| Ok(PathBuf::from(text)),
});

#[derive(Debug, uniffi::Error)]
pub enum FreshError {
    Io { message: String },
}

impl fmt::Display for FreshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FreshError::Io { message } => f.write_str(message),
        }
    }
}

impl std::error::Error for FreshError {}

impl From<std::io::Error> for FreshError {
    fn from(e: std::io::Error) -> Self {
        FreshError::Io { message: e.to_string() }
    }
}

/// A finished review: the scan's totals and everything worth a look.
#[derive(uniffi::Record)]
pub struct Review {
    pub root: PathBuf,
    pub bytes: u64,
    pub files: u64,
    pub seconds: f64,
    /// Folders left out until the app has Full Disk Access.
    pub needs_full_disk_access: Vec<PathBuf>,
    pub findings: Vec<Finding>,
    pub joints: Vec<Joint>,
}

/// Runs reviews, reports progress while one runs, and keeps the last scan for the space map.
#[derive(Default, uniffi::Object)]
pub struct Reviewer {
    entries_seen: Arc<AtomicU64>,
    last: Mutex<Option<(Scan, Atlas)>>,
}

#[uniffi::export]
impl Reviewer {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::default()
    }

    /// Directory entries listed so far by the review in progress.
    pub fn entries_seen(&self) -> u64 {
        self.entries_seen.load(Ordering::Relaxed)
    }

    /// Scans `root` and reviews it with the default thresholds. Takes seconds, so call it off
    /// the main thread. `fetch` asks each repo's remote first, so remote branches show up.
    pub fn review(&self, root: PathBuf, fetch: bool) -> Result<Review, FreshError> {
        self.entries_seen.store(0, Ordering::Relaxed);
        let opts = ScanOptions { progress: Some(self.entries_seen.clone()), ..ScanOptions::default() };
        let scan = crate::scan(&root, &opts)?;
        let crate::Review { findings, joints } =
            crate::review(&scan, &ReviewOptions { fetch, ..ReviewOptions::default() });
        let review = Review {
            root: scan.root.clone(),
            bytes: scan.node(0).bytes,
            files: scan.stats.files,
            seconds: scan.stats.elapsed.as_secs_f64(),
            needs_full_disk_access: scan.stats.needs_full_disk_access.clone(),
            findings,
            joints,
        };
        let atlas = Atlas::new(&scan, &review.findings);
        *self.last.lock().unwrap() = Some((scan, atlas));
        Ok(review)
    }

    /// The last review's folder `node` (its root when `None`) as a treemap in a `width` ×
    /// `height` box, `depth` levels deep. `None` before the first review finishes.
    pub fn space_map(&self, node: Option<u32>, width: f64, height: f64, depth: u32) -> Option<SpaceMap> {
        let last = self.last.lock().unwrap();
        let (scan, atlas) = last.as_ref()?;
        Some(atlas.map(scan, node, width, height, depth))
    }
}

#[uniffi::export]
pub fn home_folder() -> PathBuf {
    crate::home()
}

/// Space applying `findings` together frees, counting nested ones once and the joint savings
/// they complete.
#[uniffi::export]
pub fn freed_by(findings: Vec<Finding>, joints: Vec<Joint>) -> u64 {
    crate::freed(&findings, &joints)
}

/// Applies findings, journaling them where the CLI does, so either can undo the other's work.
#[uniffi::export]
pub fn apply_findings(findings: Vec<Finding>) -> Result<Vec<Applied>, FreshError> {
    Ok(apply::apply(findings, &Journal::default_location(), |_| {})?)
}

/// Puts back the last applied batch; `None` when there's nothing to undo.
#[uniffi::export]
pub fn undo_last() -> Result<Option<Undone>, FreshError> {
    Ok(apply::undo_last(&Journal::default_location())?)
}
