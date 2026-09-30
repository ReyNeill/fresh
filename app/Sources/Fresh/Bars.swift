import AppKit
import FreshCore
import SwiftUI

/// The selection's size and the button that cleans it up, after a confirmation.
struct ActionBar: View {
    @Environment(AppModel.self) private var model
    @State private var confirming = false

    var body: some View {
        let selected = model.selected
        HStack {
            Text(model.busy ? "Cleaning up…" : summary(selected)).foregroundStyle(.secondary)
            Spacer()
            Button("Clean Up…") { confirming = true }
                .keyboardShortcut(.defaultAction)
                .disabled(selected.isEmpty || model.busy || model.isScanning)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .confirmationDialog(plan(selected), isPresented: $confirming) {
            Button("Clean Up", role: .destructive) { Task { await model.cleanUp() } }
        } message: {
            Text(warning(selected))
        }
    }

    private func summary(_ selected: [Finding]) -> String {
        guard !selected.isEmpty else { return "Nothing selected" }
        let bytes = outermost(selected).map(\.bytes).reduce(0, +)
        let count = "\(selected.count) selected"
        return bytes > 0 ? "\(count) · \(bytes.formattedBytes)" : count
    }

    /// "Move 12 items (38 GB) to the Trash and delete 3 branches?"
    private func plan(_ selected: [Finding]) -> String {
        var parts: [String] = []
        let trash = selected.filter(\.movesToTrash)
        if !trash.isEmpty {
            let bytes = outermost(trash).map(\.bytes).reduce(0, +)
            parts.append("move \(plural(trash.count, "item", "items")) (\(bytes.formattedBytes)) to the Trash")
        }
        let local = selected.filter { if case .deleteBranch = $0.action { true } else { false } }.count
        if local > 0 { parts.append("delete \(plural(local, "branch", "branches"))") }
        let remote = selected.filter { if case .deleteRemoteBranch = $0.action { true } else { false } }.count
        if remote > 0 { parts.append("delete \(plural(remote, "branch", "branches")) on the remote") }
        let tools = selected.filter { !$0.movesToTrash && !$0.deletesBranch }.count
        if tools > 0 { parts.append("run \(plural(tools, "cleanup", "cleanups"))") }
        let sentence = parts.formatted(.list(type: .and))
        return sentence.prefix(1).uppercased() + sentence.dropFirst() + "?"
    }

    private func warning(_ selected: [Finding]) -> String {
        var lines = ["Undo puts back anything moved to the Trash or deleted, until you empty the Trash."]
        if selected.contains(where: { !$0.movesToTrash && !$0.deletesBranch }) {
            lines.append("Tool cleanups, like removing simulators, can't be undone.")
        }
        if selected.contains(where: { $0.safety == .remote }) {
            lines.append("Deleting remote branches affects everyone who uses those repositories.")
        }
        return lines.joined(separator: " ")
    }
}

/// The last clean-up or undo, with what was skipped and a way back.
struct NoticeBar: View {
    @Environment(AppModel.self) private var model
    let notice: Notice

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: notice.problems.isEmpty ? "checkmark.circle" : "exclamationmark.circle")
                .foregroundStyle(notice.problems.isEmpty ? .green : .orange)
            VStack(alignment: .leading, spacing: 4) {
                Text(notice.message)
                ForEach(notice.problems.prefix(3), id: \.self) { problem in
                    Text(problem).font(.callout).foregroundStyle(.secondary)
                }
                if notice.problems.count > 3 {
                    Text("and \(notice.problems.count - 3) more").font(.callout).foregroundStyle(.secondary)
                }
            }
            Spacer()
            if notice.canUndo {
                Button("Undo") { Task { await model.undo() } }
            }
            Button { model.dismissNotice() } label: { Image(systemName: "xmark") }
                .buttonStyle(.borderless)
                .help("Dismiss")
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(.bar)
    }
}

/// Shown when other apps' containers were left out of the scan.
struct FullDiskAccessBar: View {
    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "lock").foregroundStyle(.secondary)
            Text("Other apps' data isn't included. Give Fresh Full Disk Access to review it too.")
            Spacer()
            Button("Open Settings") {
                NSWorkspace.shared.open(
                    URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")!
                )
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(.bar)
    }
}
