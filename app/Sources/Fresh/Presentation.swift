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
        default: label ?? URL(filePath: path).lastPathComponent
        }
    }

    /// Where it is: the folder holding it, or the repository for branches.
    var place: String {
        tilde(deletesBranch ? path : URL(filePath: path).deletingLastPathComponent().path)
    }

    /// Why it's listed, what it shares with copies elsewhere, and how long it's been untouched.
    var reason: String {
        var parts = [detail]
        let shared = size > bytes ? size - bytes : 0
        if canApply, shared > size / 10 { parts.append("\(shared.formattedBytes) shared with copies") }
        if let idleDays, idleDays > 0 { parts.append("idle \(plural(Int(idleDays), "day", "days"))") }
        return parts.joined(separator: " · ")
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

extension Kind {
    /// Every kind, in legend order.
    static let all: [Kind] = [.code, .git, .cache, .toolchain, .synced, .media, .documents, .apps, .other]

    var title: String {
        switch self {
        case .code: "Code"
        case .git: "Git"
        case .cache: "Caches"
        case .toolchain: "Toolchains"
        case .synced: "Synced"
        case .media: "Media"
        case .documents: "Documents"
        case .apps: "Apps"
        case .other: "Other"
        }
    }

    var color: Color {
        switch self {
        case .code: Palette.mapCode
        case .git: Palette.mapGit
        case .cache: Palette.mapCache
        case .toolchain: Palette.mapToolchain
        case .synced: Palette.mapSynced
        case .media: Palette.mapMedia
        case .documents: Palette.mapDocuments
        case .apps: Palette.mapApps
        case .other: Palette.mapOther
        }
    }
}

extension Tile {
    var rect: CGRect { CGRect(x: x, y: y, width: width, height: height) }
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
