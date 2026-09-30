//! Moving things to the Trash through Foundation, so they can be put back.

use std::path::{Path, PathBuf};

use objc2::rc::{Retained, autoreleasepool};
use objc2_foundation::{NSFileManager, NSString, NSURL};

/// Moves a file or folder to the Trash and returns where it landed.
pub fn trash(path: &Path) -> Result<PathBuf, String> {
    let path = path.to_str().ok_or("path is not valid UTF-8")?;
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        let mut landed: Option<Retained<NSURL>> = None;
        NSFileManager::defaultManager()
            .trashItemAtURL_resultingItemURL_error(&url, Some(&mut landed))
            .map_err(|e| e.localizedDescription().to_string())?;
        landed
            .and_then(|url| url.path())
            .map(|p| PathBuf::from(p.to_string()))
            .ok_or_else(|| "the Trash did not say where the item went".to_owned())
    })
}
