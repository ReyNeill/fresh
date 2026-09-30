import FreshCore
import SwiftUI

/// The floating bar that holds the one action, like ChatGPT's composer: a sentence saying
/// what Clean up will do, the remote check, and the button.
struct Composer: View {
    @Environment(AppModel.self) private var model
    @State private var confirming = false

    var body: some View {
        let selected = model.selected
        VStack(alignment: .leading, spacing: 12) {
            Text(model.busy ? "Cleaning up…" : selected.isEmpty ? "Select what to clean up" : plan(selected) + ".")
                .font(.system(size: 13))
                .foregroundStyle(selected.isEmpty || model.busy ? Palette.tertiaryText : Palette.text)
                .lineLimit(2)
            HStack(spacing: 6) {
                Button { model.checkRemotes.toggle() } label: {
                    Label("Check remotes", systemImage: "icloud.and.arrow.down")
                }
                .buttonStyle(ChipButtonStyle(active: model.checkRemotes))
                .help("Fetch every repository before reviewing, so your merged branches on the remote show up. Uses the network.")
                Spacer()
                Text(count(selected))
                    .font(.system(size: 12))
                    .foregroundStyle(Palette.tertiaryText)
                    .monospacedDigit()
                Button("Clean up") { confirming = true }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                    .disabled(selected.isEmpty || model.busy || model.isScanning)
            }
        }
        .padding(.horizontal, 16)
        .padding(.top, 14)
        .padding(.bottom, 12)
        .card(radius: Radius.composer)
        .confirmationDialog(plan(selected) + "?", isPresented: $confirming) {
            Button("Clean Up", role: .destructive) { Task { await model.cleanUp() } }
        } message: {
            Text(warning(selected))
        }
    }

    private func count(_ selected: [Finding]) -> String {
        guard !selected.isEmpty else { return "" }
        let bytes = outermost(selected).map(\.bytes).reduce(0, +)
        return bytes > 0 ? "\(selected.count) selected · \(bytes.formattedBytes)" : "\(selected.count) selected"
    }

    /// "Move 12 items (38 GB) to the Trash and delete 3 branches"
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
        return sentence.prefix(1).uppercased() + sentence.dropFirst()
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

/// The last clean-up or undo, sitting above the composer like ChatGPT's status chips.
struct NoticeChip: View {
    @Environment(AppModel.self) private var model
    let notice: Notice

    var body: some View {
        HStack(alignment: .center, spacing: 8) {
            Image(systemName: notice.problems.isEmpty ? "checkmark.circle" : "exclamationmark.circle")
                .foregroundStyle(notice.problems.isEmpty ? Palette.green : Palette.amber)
            VStack(alignment: .leading, spacing: 3) {
                Text(notice.message).foregroundStyle(Palette.secondaryText)
                ForEach(notice.problems.prefix(3), id: \.self) { problem in
                    Text(problem).foregroundStyle(Palette.tertiaryText).lineLimit(1)
                }
                if notice.problems.count > 3 {
                    Text("and \(notice.problems.count - 3) more").foregroundStyle(Palette.tertiaryText)
                }
            }
            Spacer(minLength: 8)
            if notice.canUndo {
                Button("Undo") { Task { await model.undo() } }
                    .buttonStyle(ChipButtonStyle(active: true))
            }
            Button { model.dismissNotice() } label: {
                Image(systemName: "xmark").font(.system(size: 10, weight: .semibold))
            }
            .buttonStyle(ChipButtonStyle())
            .help("Dismiss")
        }
        .font(.system(size: 12))
        .padding(.leading, 12)
        .padding(.trailing, 6)
        .padding(.vertical, 6)
        .card()
    }
}
