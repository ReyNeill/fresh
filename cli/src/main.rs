//! `fresh`: find what's taking space and clutter on a Mac, review it, and clean it up
//! reversibly.

use std::collections::BTreeMap;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use fresh_core::apply::{self, Applied, Journal, Outcome, Restored};
use fresh_core::scan::NodeId;
use fresh_core::{Action, Finding, Joint, Plan, ReviewOptions, Rule, Safety, Scan, ScanOptions, freed};

#[derive(Parser)]
#[command(name = "fresh", version, about = "Find what can go on your Mac, review it, and clean up reversibly")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show where the space goes
    Scan {
        /// Folder to scan [default: home]
        path: Option<PathBuf>,
        /// Levels of folders to show
        #[arg(long, default_value_t = 2)]
        depth: usize,
        /// Largest entries shown per folder
        #[arg(long, default_value_t = 8)]
        top: usize,
    },
    /// List what can go, grouped by kind and sized
    Review {
        /// Folder to review [default: home]
        path: Option<PathBuf>,
        /// Print the plan as JSON instead of a summary
        #[arg(long)]
        json: bool,
        /// Save the plan to a file for `fresh apply`
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Hide findings smaller than this, e.g. 50M or 1G (git findings always show)
        #[arg(long, default_value = "50M", value_parser = parse_size)]
        min: u64,
        /// Only suggest build output of projects untouched for this many days
        #[arg(long, default_value_t = 14)]
        idle: u32,
        /// Fetch and prune each repo first so deleted upstream branches show up (uses the network)
        #[arg(long)]
        fetch: bool,
        /// Show every finding instead of the largest in each group
        #[arg(long)]
        all: bool,
    },
    /// Apply a saved plan: move to the Trash, remove worktrees, delete branches
    Apply {
        plan: PathBuf,
        /// Only these finding ids, comma separated
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
        /// Also delete your merged branches on remotes (affects other people)
        #[arg(long)]
        remote: bool,
        /// Don't ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
    /// Put back everything from the last apply
    Undo {
        /// Don't ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Scan { path, depth, top } => scan(path, depth, top),
        Command::Review { path, json, out, min, idle, fetch, all } => review(path, json, out, min, idle, fetch, all),
        Command::Apply { plan, only, remote, yes } => apply(&plan, &only, remote, yes),
        Command::Undo { yes } => undo(yes),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fresh: {e}");
            ExitCode::FAILURE
        }
    }
}

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn run_scan(path: Option<PathBuf>) -> Result<Scan> {
    let root = path.unwrap_or_else(fresh_core::home);
    eprintln!("{}", dim(&format!("Scanning {}…", tilde(&root))));
    Ok(fresh_core::scan(&root, &ScanOptions::default())?)
}

fn print_summary(scan: &Scan) {
    let s = &scan.stats;
    println!(
        "{} · {} in {} files · {:.1}s",
        bold(&tilde(&scan.root)),
        size(scan.node(0).bytes),
        count(s.files),
        s.elapsed.as_secs_f64()
    );
    if !s.needs_full_disk_access.is_empty() {
        let skipped: Vec<String> = s.needs_full_disk_access.iter().map(|p| tilde(p)).collect();
        println!("{}", dim(&format!("Needs Full Disk Access to include: {}", skipped.join(", "))));
    }
}

fn scan(path: Option<PathBuf>, depth: usize, top: usize) -> Result {
    let scan = run_scan(path)?;
    print_summary(&scan);
    println!();
    print_tree(&scan, 0, depth, top, 0);
    Ok(())
}

fn print_tree(scan: &Scan, id: NodeId, depth: usize, top: usize, level: usize) {
    if level == depth {
        return;
    }
    let children: Vec<NodeId> = scan.children(id).collect();
    for &child in children.iter().take(top) {
        let node = scan.node(child);
        let indent = "   ".repeat(level);
        println!("{indent}{:>10}  {}", size(node.bytes), node.name);
        print_tree(scan, child, depth, top, level + 1);
    }
    if children.len() > top {
        let rest: u64 = children[top..].iter().map(|&c| scan.node(c).bytes).sum();
        println!("{}{}", "   ".repeat(level), dim(&format!("{:>10}  … {} more", size(rest), children.len() - top)));
    }
}

fn review(
    path: Option<PathBuf>,
    json: bool,
    out: Option<PathBuf>,
    min: u64,
    idle: u32,
    fetch: bool,
    all: bool,
) -> Result {
    let scan = run_scan(path)?;
    let opts = ReviewOptions { fetch, min_bytes: min, build_output_idle_days: idle, ..ReviewOptions::default() };
    let fresh_core::Review { findings, joints } = fresh_core::review(&scan, &opts);
    let plan = Plan { root: scan.root.clone(), created: opts.now, findings, joints };

    if json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }
    print_summary(&scan);
    print_findings(&plan.findings, &plan.joints, all);
    if !fetch {
        println!(
            "{}",
            dim(
                "\nRemote branches, and local ones whose upstream was deleted, show up with --fetch (uses the network)."
            )
        );
    }
    match out {
        Some(out) => {
            std::fs::write(&out, serde_json::to_string_pretty(&plan)?)?;
            println!(
                "\nSaved {} findings to {}. Delete the ones to keep, then run `fresh apply {}`.",
                plan.findings.len(),
                out.display(),
                out.display()
            );
        }
        None => println!("\nSave a plan with `fresh review -o plan.json`, trim it, then `fresh apply plan.json`."),
    }
    Ok(())
}

/// Largest findings shown per group unless `--all`.
const GROUP_LIMIT: usize = 12;

fn print_findings(findings: &[Finding], joints: &[Joint], all: bool) {
    let mut groups: BTreeMap<Rule, Vec<&Finding>> = BTreeMap::new();
    for f in findings {
        groups.entry(f.rule).or_default().push(f);
    }
    for (rule, items) in &groups {
        let group: Vec<Finding> = items.iter().map(|&f| f.clone()).collect();
        let bytes = freed(&group, joints);
        let total = if bytes > 0 { format!("{} · {}", size(bytes), items.len()) } else { items.len().to_string() };
        println!("\n{}  {}", bold(rule.title()), dim(&total));
        let shown = if all { items.len() } else { GROUP_LIMIT.min(items.len()) };
        for f in &items[..shown] {
            print_finding(f);
        }
        if shown < items.len() {
            let rest: u64 = items[shown..].iter().map(|f| f.size).sum();
            let rest = if rest > 0 { format!(", {}", size(rest)) } else { String::new() };
            println!("{}", dim(&format!("  … {} more{rest} (--all shows them)", items.len() - shown)));
        }
    }

    let mut by_safety: BTreeMap<Safety, Vec<Finding>> = BTreeMap::new();
    for f in findings {
        by_safety.entry(f.safety).or_default().push(f.clone());
    }
    let parts: Vec<String> = by_safety
        .iter()
        .map(|(s, group)| (s, freed(group, joints)))
        .filter(|&(_, b)| b > 0)
        .map(|(s, b)| format!("{} {}", size(b), safety_label(*s)))
        .collect();
    if !parts.is_empty() {
        println!("\n{} {}", bold("Reclaimable:"), parts.join(" · "));
    }
}

fn print_finding(f: &Finding) {
    let shown = if f.size > 0 { size(f.size) } else { String::new() };
    let mut note = vec![f.detail.clone()];
    let shared = f.size.saturating_sub(f.bytes);
    if f.action.frees_space() && shared > f.size / 10 {
        note.push(format!("{} shared with copies", size(shared)));
    }
    if let Some(days) = f.idle_days {
        note.push(format!("idle {days}d"));
    }
    note.push(safety_label(f.safety).into());
    println!("  {}  {shown:>9}  {}  {}", dim(&f.id), subject(f), dim(&note.join(" · ")));
}

fn safety_label(safety: Safety) -> &'static str {
    match safety {
        Safety::Regenerable => "regenerable",
        Safety::Reversible => "reversible",
        Safety::Review => "to review",
        Safety::Remote => "remote",
    }
}

fn apply(plan_path: &Path, only: &[String], remote: bool, yes: bool) -> Result {
    let plan: Plan = serde_json::from_str(&std::fs::read_to_string(plan_path)?)?;
    let mut findings: Vec<Finding> = plan
        .findings
        .into_iter()
        .filter(|f| f.action != Action::Nothing)
        .filter(|f| only.is_empty() || only.contains(&f.id))
        .collect();
    let remote_count = findings.iter().filter(|f| f.safety == Safety::Remote).count();
    if !remote {
        findings.retain(|f| f.safety != Safety::Remote);
    }
    if findings.is_empty() {
        println!("Nothing to apply.");
        return Ok(());
    }

    print_findings(&findings, &plan.joints, true);
    if !remote && remote_count > 0 {
        println!(
            "{}",
            dim(&format!("Leaving out {remote_count} remote branch deletions; pass --remote to include them."))
        );
    }
    if !yes && !confirm(&format!("\nApply {} findings?", findings.len()))? {
        println!("Nothing changed.");
        return Ok(());
    }

    let journal = Journal::default_location();
    let results = apply::apply(findings, &journal, |Applied { finding, outcome }| {
        let target = subject(finding);
        match outcome {
            Outcome::Applied => println!("  {} {target}", green("✓")),
            Outcome::Skipped { reason } => println!("  {} {target}: {}", dim("–"), dim(reason)),
            Outcome::Failed { error } => println!("  {} {target}: {error}", red("✗")),
        }
    })?;

    let applied: Vec<Finding> =
        results.iter().filter(|a| matches!(a.outcome, Outcome::Applied)).map(|a| a.finding.clone()).collect();
    let trashed = freed(&applied, &plan.joints);
    let failed = results.iter().filter(|a| matches!(a.outcome, Outcome::Failed { .. })).count();
    println!();
    if trashed > 0 {
        println!("Moved {} to the Trash. Empty the Trash to free the space.", size(trashed));
    }
    println!("`fresh undo` puts back what this run changed.");
    if failed > 0 {
        return Err(format!("{failed} findings failed").into());
    }
    Ok(())
}

fn undo(yes: bool) -> Result {
    let journal = Journal::default_location();
    if !yes && !confirm("Put back everything from the last apply?")? {
        println!("Nothing changed.");
        return Ok(());
    }
    let Some(undone) = apply::undo_last(&journal)? else {
        println!("Nothing to undo.");
        return Ok(());
    };
    let mut failed = 0;
    for Restored { finding, error } in &undone.restored {
        match error {
            None => println!("  {} {}", green("↺"), subject(finding)),
            Some(e) => {
                failed += 1;
                println!("  {} {}: {e}", red("✗"), subject(finding));
            }
        }
    }
    if failed > 0 {
        return Err(format!("{failed} findings could not be put back").into());
    }
    Ok(())
}

/// What a finding is about: the branch for branch findings, the path for everything else.
fn subject(f: &Finding) -> String {
    let repo = |repo: &Path| repo.file_name().map_or_else(|| tilde(repo), |n| n.to_string_lossy().into_owned());
    match &f.action {
        Action::DeleteBranch { repo: r, branch, .. } => format!("{}: {branch}", repo(r)),
        Action::DeleteRemoteBranch { repo: r, remote, branch, .. } => format!("{}: {remote}/{branch}", repo(r)),
        _ => tilde(&f.path),
    }
}

fn confirm(question: &str) -> Result<bool> {
    if !io::stdin().is_terminal() {
        return Err("not a terminal; pass --yes to confirm".into());
    }
    print!("{question} [y/N] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

/// Parses sizes like `50M`, `1.5G` or `4096` (binary units).
fn parse_size(s: &str) -> std::result::Result<u64, String> {
    let s = s.trim().trim_end_matches(['B', 'b']).trim_end_matches('i');
    let (number, unit) = s.split_at(s.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(s.len()));
    let shift = match unit.to_ascii_uppercase().as_str() {
        "" => 0,
        "K" => 10,
        "M" => 20,
        "G" => 30,
        "T" => 40,
        _ => return Err(format!("unknown size unit in {s:?}")),
    };
    let value: f64 = number.parse().map_err(|_| format!("not a size: {s:?}"))?;
    Ok((value * (1u64 << shift) as f64) as u64)
}

fn size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    match unit {
        0 => format!("{bytes} B"),
        _ if value >= 100.0 => format!("{value:.0} {}", UNITS[unit]),
        _ => format!("{value:.1} {}", UNITS[unit]),
    }
}

fn count(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{:.0}k", n as f64 / 1e3),
        _ => format!("{:.2}M", n as f64 / 1e6),
    }
}

fn tilde(path: &Path) -> String {
    match path.strip_prefix(fresh_core::home()) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

fn color(code: &str, s: &str) -> String {
    if io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_owned()
    }
}

fn bold(s: &str) -> String {
    color("1", s)
}

fn dim(s: &str) -> String {
    color("2", s)
}

fn green(s: &str) -> String {
    color("32", s)
}

fn red(s: &str) -> String {
    color("31", s)
}
