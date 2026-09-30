//! Filesystem rules: build output, caches, Xcode data, models, installers, large old files.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::ReviewOptions;
use crate::finding::{Action, Finding, Rule, Safety};
use crate::scan::{Markers, NO_PARENT, Node, NodeId, NodeKind, Scan};

/// Subtrees already covered by a finding. Later rules and git discovery skip them, so
/// nothing is reported twice.
pub struct Claims(Vec<bool>);

impl Claims {
    pub fn new(scan: &Scan) -> Self {
        Self(vec![false; scan.nodes.len()])
    }

    fn claim(&mut self, id: NodeId) {
        self.0[id as usize] = true;
    }

    /// Whether `id` or one of its ancestors is claimed.
    fn covers(&self, scan: &Scan, mut id: NodeId) -> bool {
        while id != NO_PARENT {
            if self.0[id as usize] {
                return true;
            }
            id = scan.node(id).parent;
        }
        false
    }
}

/// Visits `from` and its subtree in preorder, skipping claimed subtrees. `visit` returns
/// whether to descend into the node it was given.
pub(crate) fn walk(scan: &Scan, from: NodeId, claims: &Claims, mut visit: impl FnMut(NodeId, &Node) -> bool) {
    let end = scan.node(from).end;
    let mut id = from;
    while id < end {
        let node = scan.node(id);
        id = if !claims.0[id as usize] && visit(id, node) { id + 1 } else { node.end };
    }
}

/// Folders that hold installed tools or apps rather than projects. Build-output names inside
/// them are load-bearing: a VS Code extension's `node_modules` is the extension.
const NOT_PROJECTS: &[&str] = &[
    "Library",
    ".git",
    ".Trash",
    ".vscode",
    ".vscode-insiders",
    ".vscode-server",
    ".cursor",
    ".windsurf",
    ".nvm",
    ".volta",
    ".fnm",
    ".bun",
    ".npm",
    ".cargo",
    ".rustup",
    ".local",
    ".pyenv",
    ".rbenv",
    ".gem",
    ".deno",
];

/// Whether a folder belongs to a tool or app install rather than a project.
pub(crate) fn is_tool_dir(name: &str) -> bool {
    NOT_PROJECTS.contains(&name) || name.ends_with(".app")
}

pub(crate) fn review(scan: &Scan, opts: &ReviewOptions, claims: &mut Claims) -> Vec<Finding> {
    let mut out = Vec::new();
    known_locations(scan, opts, claims, &mut out);
    simulators(scan, opts, claims, &mut out);
    build_outputs(scan, opts, claims, &mut out);
    installers(scan, opts, claims, &mut out);
    large_old_files(scan, opts, claims, &mut out);
    out
}

enum How {
    Trash,
    Run(&'static [&'static str]),
    Nothing,
}

struct Known {
    /// Path relative to the home folder.
    rel: &'static str,
    rule: Rule,
    safety: Safety,
    what: &'static str,
    /// Report each entry inside separately (one per app cache) instead of the whole folder.
    per_child: bool,
    how: How,
}

const fn known(rel: &'static str, rule: Rule, safety: Safety, what: &'static str) -> Known {
    Known { rel, rule, safety, what, per_child: false, how: How::Trash }
}

/// Well-known locations, most specific first: an earlier entry claims its folder, so a later
/// per-child entry for the parent skips it.
const KNOWN: &[Known] = &[
    known("Library/Developer/Xcode/DerivedData", Rule::Xcode, Safety::Regenerable, "Xcode build products"),
    known(
        "Library/Developer/Xcode/iOS DeviceSupport",
        Rule::Xcode,
        Safety::Regenerable,
        "Debug symbols from devices, copied again when a device connects",
    ),
    known(
        "Library/Developer/Xcode/watchOS DeviceSupport",
        Rule::Xcode,
        Safety::Regenerable,
        "Debug symbols from watches, copied again when a watch connects",
    ),
    known(
        "Library/Developer/Xcode/Archives",
        Rule::Xcode,
        Safety::Review,
        "App archives; keep any you may need to symbolicate crashes",
    ),
    known("Library/Developer/CoreSimulator/Caches", Rule::Xcode, Safety::Regenerable, "Simulator caches"),
    known(".cache/huggingface", Rule::Model, Safety::Review, "Hugging Face models and datasets"),
    known(".ollama/models", Rule::Model, Safety::Review, "Ollama models"),
    known(".lmstudio/models", Rule::Model, Safety::Review, "LM Studio models"),
    known("Library/Application Support/MobileSync/Backup", Rule::Backup, Safety::Review, "iPhone and iPad backups"),
    known(".cargo/registry", Rule::Cache, Safety::Regenerable, "Cargo crate downloads"),
    known(".cargo/git", Rule::Cache, Safety::Regenerable, "Cargo git dependencies"),
    known(".bun/install/cache", Rule::Cache, Safety::Regenerable, "Bun package cache"),
    known(".npm/_cacache", Rule::Cache, Safety::Regenerable, "npm package cache"),
    known(".gradle/caches", Rule::Cache, Safety::Regenerable, "Gradle caches"),
    known(
        "Library/Caches/ms-playwright",
        Rule::Cache,
        Safety::Review,
        "Browsers for Playwright; tests need `playwright install` again",
    ),
    known(".cache/puppeteer", Rule::Cache, Safety::Review, "Browsers for Puppeteer; reinstall them after"),
    Known {
        how: How::Run(&["go", "clean", "-modcache"]),
        ..known("go/pkg/mod", Rule::Cache, Safety::Regenerable, "Go module cache")
    },
    Known { per_child: true, ..known("Library/Caches", Rule::Cache, Safety::Regenerable, "App cache") },
    Known { per_child: true, ..known(".cache", Rule::Cache, Safety::Regenerable, "Tool cache") },
    Known {
        how: How::Nothing,
        ..known(".Trash", Rule::Trash, Safety::Review, "Already in the Trash; empty it to free the space")
    },
];

fn known_locations(scan: &Scan, opts: &ReviewOptions, claims: &mut Claims, out: &mut Vec<Finding>) {
    let mut running: Option<Vec<String>> = None;
    for k in KNOWN {
        let Some(id) = scan.find(&opts.home.join(k.rel)) else { continue };
        if claims.covers(scan, id) {
            continue;
        }
        let targets: Vec<NodeId> = if k.per_child {
            // macOS manages its own caches and purges them under disk pressure.
            scan.children(id)
                .filter(|&c| !claims.0[c as usize] && !scan.node(c).name.starts_with("com.apple."))
                .collect()
        } else {
            vec![id]
        };
        for target in targets {
            let node = scan.node(target);
            if node.bytes == 0 {
                continue;
            }
            let path = scan.path(target);
            let action = match k.how {
                How::Trash => Action::Trash { path: path.clone() },
                How::Run(argv) => {
                    Action::Run { cwd: opts.home.clone(), argv: argv.iter().map(|s| s.to_string()).collect() }
                }
                How::Nothing => Action::Nothing,
            };
            let running = running.get_or_insert_with(running_programs);
            let (safety, what) = if k.per_child && in_use(&node.name, running) {
                (Safety::Review, format!("{}; its app is running, so quit it first", k.what))
            } else {
                (k.safety, k.what.to_owned())
            };
            out.push(
                Finding::new(k.rule, safety, path, node.bytes, what, action)
                    .idle(Some(crate::days(opts.now, node.newest))),
            );
            claims.claim(target);
        }
    }
}

/// Executable paths of running processes, lowercased.
pub(crate) fn running_programs() -> Vec<String> {
    Command::new("ps")
        .args(["-axo", "comm="])
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(str::to_lowercase).collect())
        .unwrap_or_default()
}

/// Whether a cache folder likely belongs to a running program: a distinctive word of its
/// name (`Google`, `com.linear.ShipIt` → `linear`) appears in a running executable's path.
fn in_use(cache: &str, running: &[String]) -> bool {
    const GENERIC: &[&str] = &["cache", "caches", "updater", "desktop", "helper", "shipit"];
    cache
        .split(['.', '-', '_', '@', ' '])
        .map(str::to_lowercase)
        .filter(|word| word.len() >= 5 && !GENERIC.contains(&word.as_str()))
        .any(|word| running.iter().any(|program| program.contains(&word)))
}

/// Simulator devices whose runtime is gone can't boot anymore; `simctl` removes them.
fn simulators(scan: &Scan, opts: &ReviewOptions, claims: &mut Claims, out: &mut Vec<Finding>) {
    #[derive(Deserialize)]
    struct List {
        devices: HashMap<String, Vec<Device>>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Device {
        udid: String,
        is_available: bool,
    }

    let devices_dir = opts.home.join("Library/Developer/CoreSimulator/Devices");
    // Only ask xcrun when Xcode has been used: without it, xcrun pops an install dialog.
    let Some(dir) = scan.find(&devices_dir) else { return };
    let Ok(output) = Command::new("xcrun")
        .args(["simctl", "list", "devices", "--json"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    else {
        return;
    };
    let Ok(list) = serde_json::from_slice::<List>(&output.stdout) else { return };
    let gone: Vec<NodeId> =
        list.devices.values().flatten().filter(|d| !d.is_available).filter_map(|d| scan.child(dir, &d.udid)).collect();
    if gone.is_empty() {
        return;
    }
    let bytes = gone.iter().map(|&id| scan.node(id).bytes).sum();
    out.push(Finding::new(
        Rule::Xcode,
        Safety::Regenerable,
        devices_dir,
        bytes,
        format!("{} simulators whose runtime is no longer installed", gone.len()),
        Action::Run {
            cwd: opts.home.clone(),
            argv: ["xcrun", "simctl", "delete", "unavailable"].map(String::from).to_vec(),
        },
    ));
    for id in gone {
        claims.claim(id);
    }
}

/// Folders a project regenerates, with the marker their project folder must have.
const BUILD_OUTPUTS: &[(&str, Markers, &str)] = &[
    ("target", Markers::CARGO, "Cargo build output"),
    ("node_modules", Markers::NODE, "JavaScript dependencies"),
    (".next", Markers::NODE, "Next.js build output"),
    (".turbo", Markers::NODE, "Turborepo cache"),
    (".svelte-kit", Markers::NODE, "SvelteKit build output"),
    (".nuxt", Markers::NODE, "Nuxt build output"),
    (".parcel-cache", Markers::NODE, "Parcel cache"),
    (".venv", Markers::PYTHON, "Python virtual environment"),
    (".tox", Markers::PYTHON, "tox environments"),
    (".mypy_cache", Markers::PYTHON, "mypy cache"),
    (".pytest_cache", Markers::PYTHON, "pytest cache"),
    (".ruff_cache", Markers::PYTHON, "Ruff cache"),
    (".build", Markers::SWIFTPM, "Swift package build output"),
    (".gradle", Markers::GRADLE, "Gradle project cache"),
    ("build", Markers::GRADLE, "Gradle build output"),
];

fn build_output_kind(scan: &Scan, node: &Node) -> Option<&'static str> {
    let parent = if node.parent == NO_PARENT { Markers::default() } else { scan.node(node.parent).markers };
    BUILD_OUTPUTS
        .iter()
        .find(|(name, marker, _)| *name == &*node.name && parent.intersects(*marker))
        .map(|(_, _, what)| *what)
        // Tools also tag their own environments (uv tags every venv), so only trust the tag
        // inside a project.
        .or_else(|| {
            (parent.intersects(Markers::PROJECT) && node.markers.intersects(Markers::CACHEDIR_TAG))
                .then_some("Tagged cache folder")
        })
}

fn build_outputs(scan: &Scan, opts: &ReviewOptions, claims: &mut Claims, out: &mut Vec<Finding>) {
    let home = scan.find(&opts.home);
    let mut found = Vec::new();
    walk(scan, 0, claims, |id, node| {
        // Dotfolders in the home folder hold tools' own installs and state, not projects.
        let tool_state = Some(node.parent) == home && node.name.starts_with('.');
        if node.kind == NodeKind::File || tool_state || is_tool_dir(&node.name) {
            return false;
        }
        match build_output_kind(scan, node) {
            Some(what) => {
                found.push((id, what));
                false
            }
            None => true,
        }
    });
    for (id, what) in found {
        claims.claim(id);
        let path = scan.path(id);
        out.push(
            Finding::new(
                Rule::BuildOutput,
                Safety::Regenerable,
                path.clone(),
                scan.node(id).bytes,
                what.into(),
                Action::Trash { path },
            )
            .idle(project_idle(scan, opts.now, id)),
        );
    }
}

/// Days since anything in a build output's project changed, not counting build outputs or
/// `.git`, which builds and fetches touch on their own.
fn project_idle(scan: &Scan, now: i64, output: NodeId) -> Option<u32> {
    let project = scan.node(output).parent;
    if project == NO_PARENT {
        return None;
    }
    let newest = scan
        .children(project)
        .filter(|&c| {
            let node = scan.node(c);
            node.files > 0 && &*node.name != ".git" && build_output_kind(scan, node).is_none()
        })
        .map(|c| scan.node(c).newest)
        .fold(scan.node(project).own_newest, i64::max);
    Some(crate::days(now, newest))
}

const INSTALLER_EXTENSIONS: &[&str] = &["dmg", "pkg", "mpkg", "iso", "xip"];

fn installers(scan: &Scan, opts: &ReviewOptions, claims: &mut Claims, out: &mut Vec<Finding>) {
    let Some(downloads) = scan.find(&opts.home.join("Downloads")) else { return };
    let mut found = Vec::new();
    walk(scan, downloads, claims, |id, node| {
        let is_installer = Path::new(&*node.name)
            .extension()
            .is_some_and(|ext| INSTALLER_EXTENSIONS.iter().any(|i| ext.eq_ignore_ascii_case(i)));
        if node.kind == NodeKind::File && is_installer && crate::days(opts.now, node.newest) >= opts.installer_days {
            found.push(id);
        }
        true
    });
    for id in found {
        claims.claim(id);
        let path = scan.path(id);
        let days = crate::days(opts.now, scan.node(id).newest);
        out.push(
            Finding::new(
                Rule::Installer,
                Safety::Review,
                path.clone(),
                scan.node(id).bytes,
                format!("Installer downloaded {days} days ago"),
                Action::Trash { path },
            )
            .idle(Some(days)),
        );
    }
}

/// Folders Finder shows as a single document. Files inside are the document's internals and
/// must never be suggested on their own.
const PACKAGE_EXTENSIONS: &[&str] = &[
    "app",
    "photoslibrary",
    "musiclibrary",
    "tvlibrary",
    "imovielibrary",
    "aplibrary",
    "fcpbundle",
    "logicx",
    "band",
    "sparsebundle",
    "utm",
    "vmwarevm",
    "pvm",
    "xcarchive",
    "dSYM",
    "bundle",
    "framework",
];

fn large_old_files(scan: &Scan, opts: &ReviewOptions, claims: &mut Claims, out: &mut Vec<Finding>) {
    let library = scan.find(&opts.home.join("Library"));
    let mut found = Vec::new();
    walk(scan, 0, claims, |id, node| match node.kind {
        NodeKind::Dir => {
            let is_package = Path::new(&*node.name)
                .extension()
                .is_some_and(|ext| PACKAGE_EXTENSIONS.iter().any(|p| ext.eq_ignore_ascii_case(p)));
            Some(id) != library && &*node.name != ".git" && !is_package
        }
        NodeKind::File => {
            if node.bytes >= opts.large_file && crate::days(opts.now, node.newest) >= opts.large_file_days {
                found.push(id);
            }
            false
        }
    });
    for id in found {
        claims.claim(id);
        let path = scan.path(id);
        let days = crate::days(opts.now, scan.node(id).newest);
        out.push(
            Finding::new(
                Rule::LargeFile,
                Safety::Review,
                path.clone(),
                scan.node(id).bytes,
                format!("Not modified in {days} days"),
                Action::Trash { path },
            )
            .idle(Some(days)),
        );
    }
}
