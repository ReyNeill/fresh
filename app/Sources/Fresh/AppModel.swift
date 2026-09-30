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
    /// The home folder: reviewing it reviews the Mac. Launching with `-root <path>` reviews
    /// another folder instead, for trying the app on a scratch folder.
    let root: URL
    /// Fetch every repository before reviewing, so merged branches on the remote show up.
    var checkRemotes = false
    private(set) var phase: Phase = .scanning(entries: 0)
    /// Ids of the findings to clean up.
    var selection: Set<String> = []
    /// The group shown in the main column; `nil` shows everything.
    var group: Rule?
    /// The space map is shown instead of the findings.
    var showingMap = false
    /// The map of the folder being looked at, laid out for the map view's size.
    private(set) var spaceMap: SpaceMap?
    /// The map was drawn before the last clean-up, so it still shows what went.
    private(set) var mapIsStale = false
    private var mapNode: UInt32?
    private var mapSize: CGSize = .zero
    private(set) var notice: Notice?
    /// A clean-up or undo is running.
    private(set) var busy = false
    /// Paths reviews never suggest, listed in Settings.
    private(set) var excluded: [String] = []
    /// Why exclusions couldn't be read or saved, shown in Settings.
    private(set) var settingsError: String?

    private let reviewer = Reviewer()

    init() {
        root = URL(filePath: UserDefaults.standard.string(forKey: "root") ?? homeFolder())
        loadExcluded()
    }

    func loadExcluded() {
        do {
            excluded = try excludedPaths()
            settingsError = nil
        } catch {
            settingsError = error.localizedDescription
        }
    }

    /// Stops suggesting a finding, now and in every later review.
    func exclude(_ finding: Finding) {
        do {
            try FreshCore.exclude(path: finding.path)
            loadExcluded()
            if let review {
                phase = .reviewed(review.without([finding.id]))
                selection.remove(finding.id)
                forgetEmptyGroup()
            }
            notice = Notice(
                message: "\(finding.title) won't be suggested again. Settings lists everything you've excluded.",
                problems: [],
                canUndo: false
            )
        } catch {
            notice = Notice(message: error.localizedDescription, problems: [], canUndo: false)
        }
    }

    /// Lets reviews suggest a path again, from the next scan.
    func stopExcluding(_ path: String) {
        do {
            try FreshCore.stopExcluding(path: path)
            loadExcluded()
        } catch {
            settingsError = error.localizedDescription
        }
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

    /// Space cleaning up `findings` together frees: nested ones counted once, plus data their
    /// copies share when every copy is among them.
    func frees(_ findings: [Finding]) -> UInt64 {
        freedBy(findings: findings, joints: review?.joints ?? [])
    }

    /// Findings grouped by rule, in the order the review sorted them.
    var groups: [FindingGroup] {
        var groups: [FindingGroup] = []
        for finding in review?.findings ?? [] {
            if groups.last?.rule == finding.rule {
                groups[groups.count - 1].findings.append(finding)
            } else {
                groups.append(FindingGroup(rule: finding.rule, findings: [finding]))
            }
        }
        for index in groups.indices { groups[index].bytes = frees(groups[index].findings) }
        return groups
    }

    var visibleGroups: [FindingGroup] {
        guard let group else { return groups }
        return groups.filter { $0.rule == group }
    }

    /// Scans and reviews the Mac, counting listed entries as it goes.
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
            forgetEmptyGroup()
            mapNode = nil
            mapIsStale = false
            layOutMap()
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
            forgetEmptyGroup()
            if !done.isEmpty { mapIsStale = true }
            notice = Notice(applied: results, freed: frees(results.filter { $0.outcome == .applied }.map(\.finding)))
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

    /// Lays the map out again for a new view size.
    func layOutMap(in size: CGSize) {
        mapSize = size
        layOutMap()
    }

    /// Shows a folder of the scan on the map; `nil` is the scanned folder itself.
    func zoom(to node: UInt32?) {
        mapNode = node
        layOutMap()
    }

    private func layOutMap() {
        guard mapSize.width > 0, mapSize.height > 0 else { return }
        spaceMap = reviewer.spaceMap(node: mapNode, width: mapSize.width, height: mapSize.height, depth: 4)
    }

    /// The finding a map tile is, or the one it sits inside.
    func finding(covering path: String) -> Finding? {
        review?.findings.first { path == $0.path || path.hasPrefix($0.path + "/") }
    }

    /// Falls back to everything once the shown group has nothing left.
    private func forgetEmptyGroup() {
        if let group, !groups.contains(where: { $0.rule == group }) { self.group = nil }
    }
}

/// Findings of one rule.
struct FindingGroup: Identifiable {
    let rule: Rule
    var findings: [Finding]
    var id: Rule { rule }

    /// Space cleaning up the whole group frees.
    var bytes: UInt64 = 0

    /// Size when it frees space, otherwise how many there are (branches free none).
    var total: String {
        bytes > 0 ? bytes.formattedBytes : "\(findings.count)"
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
            findings: findings.filter { !ids.contains($0.id) },
            // Savings that needed a finding that's gone can't happen anymore.
            joints: joints.filter { $0.findings.allSatisfy { !ids.contains($0) } }
        )
    }
}

extension Notice {
    /// `freed` is what emptying the Trash will free for the findings that were applied.
    init(applied results: [Applied], freed trashed: UInt64) {
        let done = results.filter { $0.outcome == .applied }.map(\.finding)
        var parts: [String] = []
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
