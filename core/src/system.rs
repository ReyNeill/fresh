//! What's worth cleaning outside the home folder: old simulator runtimes and extra copies of
//! Xcode. Only whole-Mac reviews look here (`ReviewOptions::system`).

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::finding::{Action, Finding, Rule, Safety};
use crate::{ReviewOptions, ScanOptions};

/// Simulator runtimes unused this long are suggested.
const RUNTIME_IDLE_DAYS: u32 = 30;

pub(crate) fn review(opts: &ReviewOptions) -> Vec<Finding> {
    let mut out = runtimes(opts);
    out.extend(extra_xcodes());
    out
}

fn runtimes(opts: &ReviewOptions) -> Vec<Finding> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Runtime {
        identifier: String,
        platform_identifier: String,
        version: String,
        size_bytes: u64,
        last_used_at: Option<String>,
        #[serde(default)]
        deletable: bool,
        mount_path: Option<PathBuf>,
    }
    // Only ask once Xcode has set simulators up: without Xcode, xcrun offers to install it.
    if !Path::new("/Library/Developer/CoreSimulator").exists() {
        return Vec::new();
    }
    let Ok(output) = run(&["xcrun", "simctl", "runtime", "list", "-j"]) else { return Vec::new() };
    let Ok(list) = serde_json::from_str::<HashMap<String, Runtime>>(&output) else { return Vec::new() };
    list.into_values()
        .filter(|r| r.deletable)
        .filter_map(|r| {
            let idle = crate::days(opts.now, unix_time(r.last_used_at.as_deref()?)?);
            if idle < RUNTIME_IDLE_DAYS {
                return None;
            }
            let name = format!("{} {} Simulator", platform(&r.platform_identifier), r.version);
            let path = r.mount_path.unwrap_or_else(|| PathBuf::from("/Library/Developer/CoreSimulator"));
            let argv = ["xcrun", "simctl", "runtime", "delete", &r.identifier].map(String::from).to_vec();
            Some(
                Finding::new(
                    Rule::Xcode,
                    Safety::Review,
                    path,
                    r.size_bytes,
                    "Simulator runtime; Xcode can download it again".into(),
                    Action::Run { cwd: opts.home.clone(), argv },
                )
                .labeled(name)
                .idle(Some(idle)),
            )
        })
        .collect()
}

fn platform(identifier: &str) -> &str {
    match identifier {
        "com.apple.platform.iphonesimulator" => "iOS",
        "com.apple.platform.watchsimulator" => "watchOS",
        "com.apple.platform.appletvsimulator" => "tvOS",
        "com.apple.platform.xrsimulator" => "visionOS",
        other => other,
    }
}

fn extra_xcodes() -> Vec<Finding> {
    let Ok(entries) = fs::read_dir("/Applications") else { return Vec::new() };
    let installed: Vec<Xcode> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("Xcode") && n.ends_with(".app"))
        })
        .filter_map(|p| {
            let plist = p.join("Contents/Info.plist");
            let version =
                run(&["plutil", "-extract", "CFBundleShortVersionString", "raw", "-o", "-", plist.to_str()?]).ok()?;
            Some((p, version.trim().to_owned()))
        })
        .collect();
    let selected = run(&["xcode-select", "-p"]).map(|s| PathBuf::from(s.trim())).unwrap_or_default();
    let (extras, kept) = extras(&installed, &selected);
    let kept = kept.iter().map(|(_, version)| version.as_str()).collect::<Vec<_>>().join(" and ");
    let running = crate::rules::running_programs();
    extras
        .into_iter()
        .filter(|(path, _)| {
            let app = path.to_string_lossy().to_lowercase();
            !running.iter().any(|program| program.starts_with(&app))
        })
        .filter_map(|(path, version)| {
            let bytes = crate::scan(path, &ScanOptions::default()).ok()?.node(0).bytes;
            // App Store copies belong to the system, and moving them needs your password.
            let yours = fs::metadata(path).is_ok_and(|m| m.uid() == unsafe { libc::getuid() });
            let (detail, action) = if yours {
                (format!("An extra copy of Xcode; you keep {kept}"), Action::Trash { path: path.clone() })
            } else {
                (
                    format!("An extra copy of Xcode; you keep {kept}. Installed by the system, so delete it in Finder"),
                    Action::Nothing,
                )
            };
            Some(
                Finding::new(Rule::Xcode, Safety::Review, path.clone(), bytes, detail, action)
                    .labeled(format!("Xcode {version}")),
            )
        })
        .collect()
}

/// An installed copy of Xcode and its version.
type Xcode = (PathBuf, String);

/// Splits installed Xcode copies into extras worth suggesting and the ones to keep: the one
/// `xcode-select` points at, and the newest.
fn extras<'a>(installed: &'a [Xcode], selected: &Path) -> (Vec<&'a Xcode>, Vec<&'a Xcode>) {
    let newest = installed.iter().max_by(|a, b| compare_versions(&a.1, &b.1)).map(|(path, _)| path);
    installed.iter().partition(|(path, _)| !selected.starts_with(path) && Some(path) != newest)
}

fn compare_versions(a: &str, b: &str) -> Ordering {
    let parts = |v: &str| v.split('.').map(|p| p.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>();
    parts(a).cmp(&parts(b))
}

/// Unix seconds from a UTC timestamp like `2026-07-26T10:49:53Z`.
fn unix_time(timestamp: &str) -> Option<i64> {
    let (date, time) = timestamp.trim_end_matches('Z').split_once('T')?;
    let mut date = date.split('-').map(|p| p.parse::<i64>().ok());
    let (year, month, day) = (date.next()??, date.next()??, date.next()??);
    let mut time = time.split(':');
    let (hour, minute) = (time.next()?.parse::<i64>().ok()?, time.next()?.parse::<i64>().ok()?);
    let second = time.next()?.split('.').next()?.parse::<i64>().ok()?;
    // Days since 1970-01-01 (Howard Hinnant's days_from_civil).
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

fn run(argv: &[&str]) -> Result<String, ()> {
    let (program, args) = argv.split_first().ok_or(())?;
    let out = Command::new(program).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().map_err(drop)?;
    if !out.status.success() {
        return Err(());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_selected_and_the_newest_xcode() {
        let installed = [
            ("/Applications/Xcode-25.app", "25.3"),
            ("/Applications/Xcode.app", "27.0"),
            ("/Applications/Xcode-beta.app", "27.1"),
        ]
        .map(|(path, version)| (PathBuf::from(path), version.to_owned()));
        let (extra, kept) = extras(&installed, Path::new("/Applications/Xcode.app/Contents/Developer"));
        assert_eq!(extra.iter().map(|(p, _)| p.to_str().unwrap()).collect::<Vec<_>>(), ["/Applications/Xcode-25.app"]);
        assert_eq!(kept.len(), 2);

        // A single copy, selected or not, is never extra.
        let (extra, _) = extras(&installed[..1], Path::new(""));
        assert!(extra.is_empty());
    }

    #[test]
    fn reads_simctl_timestamps() {
        assert_eq!(unix_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(unix_time("2026-07-26T10:49:53Z"), Some(1_785_062_993));
        assert_eq!(unix_time("2024-02-29T23:59:59.250Z"), Some(1_709_251_199));
        assert_eq!(unix_time("yesterday"), None);
    }
}
