import Foundation
import FreshCore
import Observation

/// What the window shows.
enum Phase {
    case scanning(entries: UInt64)
    case reviewed(Review)
    case failed(String)
}

/// The outcome of the last clean-up or undo, shown above the list.
struct Notice {
    let message: String
    /// Findings that were skipped or failed, and why.
    let problems: [String]
    let canUndo: Bool
}

/// The window's state: which folder, its review, what's selected, and the last outcome.
/// All Rust calls run off the main actor; they block for as long as the work takes.
@MainActor @Observable
final class AppModel {
    private(set) var root: URL
    /// Fetch every repository before reviewing, so merged branches on the remote show up.
    var checkRemotes = false
    private(set) var phase: Phase = .scanning(entries: 0)
    /// Ids of the findings to clean up.
    var selection: Set<String> = []
    private(set) var notice: Notice?
    /// A clean-up or undo is running.
    private(set) var busy = false

    private let reviewer = Reviewer()

    init() {
        root = URL(filePath: UserDefaults.standard.string(forKey: "root") ?? homeFolder())
    }

    var review: Review? {
        if case .reviewed(let review) = phase { review } else { nil }
    }

    var isScanning: Bool {
        if case .scanning = phase { true } else { false }
    }

    var selected: [Finding] {
        review?.findings.filter { selection.contains($0.id) } ?? []
    }

    /// Reviews another folder from now on.
    func choose(_ folder: URL) async {
        root = folder
        UserDefaults.standard.set(folder.path, forKey: "root")
        await scan()
    }

    /// Scans and reviews the folder, counting listed entries as it goes.
    func scan() async {
        phase = .scanning(entries: 0)
        let reviewer = reviewer
        let path = root.path
        let fetch = checkRemotes
        let work = Task.detached(priority: .userInitiated) { try reviewer.review(root: path, fetch: fetch) }
        // A live count instead of a spinner: it changes only a few times a second.
        let progress = Task {
            while !Task.isCancelled {
                if isScanning { phase = .scanning(entries: reviewer.entriesSeen()) }
                try? await Task.sleep(for: .milliseconds(250))
            }
        }
        defer { progress.cancel() }
        do {
            let review = try await work.value
            selection = Set(review.findings.filter(\.selectedByDefault).map(\.id))
            phase = .reviewed(review)
        } catch {
            phase = .failed(error.localizedDescription)
        }
    }

    /// Applies the selected findings and drops the ones that went from the list.
    func cleanUp() async {
        guard let review else { return }
        let findings = selected
        busy = true
        defer { busy = false }
        do {
            let results = try await Task.detached { try applyFindings(findings: findings) }.value
            let done = Set(results.filter { $0.outcome == .applied }.map(\.finding.id))
            phase = .reviewed(review.without(done))
            selection.subtract(done)
            notice = Notice(applied: results)
        } catch {
            notice = Notice(message: error.localizedDescription, problems: [], canUndo: false)
        }
    }

    /// Puts back the last clean-up, from this app or the CLI, then reviews again.
    func undo() async {
        busy = true
        defer { busy = false }
        do {
            let undone = try await Task.detached { try undoLast() }.value
            notice = Notice(undone: undone)
            await scan()
        } catch {
            notice = Notice(message: error.localizedDescription, problems: [], canUndo: false)
        }
    }

    func dismissNotice() {
        notice = nil
    }
}

extension Review {
    func without(_ ids: Set<String>) -> Review {
        Review(
            root: root,
            bytes: bytes,
            files: files,
            seconds: seconds,
            needsFullDiskAccess: needsFullDiskAccess,
            findings: findings.filter { !ids.contains($0.id) }
        )
    }
}

extension Notice {
    init(applied results: [Applied]) {
        let done = results.filter { $0.outcome == .applied }.map(\.finding)
        var parts: [String] = []
        let trashed = outermost(done.filter(\.movesToTrash)).map(\.bytes).reduce(0, +)
        if trashed > 0 { parts.append("Moved \(trashed.formattedBytes) to the Trash") }
        let branches = done.filter(\.deletesBranch).count
        if branches > 0 { parts.append("deleted \(plural(branches, "branch", "branches"))") }
        let message = parts.isEmpty ? "Nothing was cleaned up." : parts.joined(separator: " and ") + "."
        self.init(
            message: trashed > 0 ? message + " Empty the Trash to free the space." : message,
            problems: results.compactMap { result in
                switch result.outcome {
                case .applied: nil
                case .skipped(let reason): "\(result.finding.title): \(reason)"
                case .failed(let error): "\(result.finding.title): \(error)"
                }
            },
            canUndo: !done.isEmpty
        )
    }

    init(undone: Undone?) {
        guard let undone else {
            self.init(message: "Nothing to undo.", problems: [], canUndo: false)
            return
        }
        let restored = undone.restored.filter { $0.error == nil }.count
        self.init(
            message: "Put back \(plural(restored, "item", "items")).",
            problems: undone.restored.compactMap { r in r.error.map { "\(r.finding.title): \($0)" } },
            canUndo: false
        )
    }
}
