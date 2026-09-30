import FreshCore
import SwiftUI

extension Finding: Identifiable {}

extension Finding {
    /// Informational findings (like a dirty worktree) have nothing to apply.
    var canApply: Bool { action != .nothing }

    /// Pre-selected: things that come back on their own or that undo restores.
    var selectedByDefault: Bool {
        canApply && (safety == .regenerable || safety == .reversible)
    }

    var movesToTrash: Bool {
        switch action {
        case .trash, .removeWorktree: true
        default: false
        }
    }

    var deletesBranch: Bool {
        switch action {
        case .deleteBranch, .deleteRemoteBranch: true
        default: false
        }
    }

    var title: String {
        switch action {
        case .deleteBranch(_, let branch, _): branch
        case .deleteRemoteBranch(_, let remote, let branch, _): "\(remote)/\(branch)"
        default: URL(filePath: path).lastPathComponent
        }
    }

    /// Where it is: the folder holding it, or the repository for branches.
    var place: String {
        tilde(deletesBranch ? path : URL(filePath: path).deletingLastPathComponent().path)
    }

    /// Why it's listed, and how long it's been untouched.
    var reason: String {
        guard let idleDays, idleDays > 0 else { return detail }
        return "\(detail) · idle \(plural(Int(idleDays), "day", "days"))"
    }
}

extension Rule {
    var title: String {
        switch self {
        case .buildOutput: "Build output"
        case .cache: "Caches"
        case .xcode: "Xcode"
        case .model: "ML models"
        case .backup: "Device backups"
        case .installer: "Old installers"
        case .largeFile: "Large old files"
        case .trash: "Trash"
        case .worktree: "Git worktrees"
        case .branch: "Git branches"
        case .remoteBranch: "Remote branches"
        }
    }

    var symbol: String {
        switch self {
        case .buildOutput: "hammer"
        case .cache: "archivebox"
        case .xcode: "iphone"
        case .model: "cpu"
        case .backup: "externaldrive"
        case .installer: "shippingbox"
        case .largeFile: "doc"
        case .trash: "trash"
        case .worktree: "folder.badge.gearshape"
        case .branch: "arrow.triangle.branch"
        case .remoteBranch: "network"
        }
    }
}

extension Safety {
    var label: String {
        switch self {
        case .regenerable: "Regenerable"
        case .reversible: "Reversible"
        case .review: "Review"
        case .remote: "Remote"
        }
    }

    var color: Color {
        switch self {
        case .regenerable: Palette.green
        case .reversible: Palette.blue
        case .review: Palette.amber
        case .remote: Palette.red
        }
    }
}

extension Safety {
    /// How a total of this safety reads after its size: "56 GB regenerable".
    var phrase: String {
        switch self {
        case .regenerable: "regenerable"
        case .reversible: "reversible"
        case .review: "to review"
        case .remote: "on remotes"
        }
    }
}

extension UInt64 {
    var formattedBytes: String { Int64(self).formatted(.byteCount(style: .file)) }
}

func plural(_ count: Int, _ one: String, _ many: String) -> String {
    "\(count) \(count == 1 ? one : many)"
}

/// A path with the home folder shown as `~`.
func tilde(_ path: String) -> String {
    let home = homeFolder()
    if path == home { return "~" }
    return path.hasPrefix(home + "/") ? "~" + path.dropFirst(home.count) : path
}

/// A folder's display name: "Home" for the home folder, else its last component.
func folderName(_ path: String) -> String {
    path == homeFolder() ? "Home" : URL(filePath: path).lastPathComponent
}

/// Findings that free space and aren't inside another one, so nested ones count once.
func outermost(_ findings: [Finding]) -> [Finding] {
    let freeing = findings.filter { $0.bytes > 0 && $0.canApply }.sorted { $0.path.count < $1.path.count }
    var kept: [Finding] = []
    for finding in freeing where !kept.contains(where: { finding.path.hasPrefix($0.path + "/") }) {
        kept.append(finding)
    }
    return kept
}
