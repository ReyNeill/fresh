import FreshCore
import SwiftUI

struct ContentView: View {
    @Environment(AppModel.self) private var model
    @State private var choosingFolder = false

    var body: some View {
        @Bindable var model = model
        VStack(spacing: 0) {
            if let notice = model.notice {
                NoticeBar(notice: notice)
                Divider()
            }
            if let review = model.review, !review.needsFullDiskAccess.isEmpty {
                FullDiskAccessBar()
                Divider()
            }
            content.frame(maxWidth: .infinity, maxHeight: .infinity)
            if model.review.map({ !$0.findings.isEmpty }) ?? false {
                Divider()
                ActionBar()
            }
        }
        .navigationTitle("Fresh")
        .navigationSubtitle(subtitle)
        .toolbar {
            ToolbarItemGroup {
                Button { choosingFolder = true } label: {
                    Label(model.root.lastPathComponent, systemImage: "folder")
                }
                .labelStyle(.titleAndIcon)
                .help("Choose the folder to review")
                Toggle(isOn: $model.checkRemotes) {
                    Label("Check remotes", systemImage: "icloud.and.arrow.down")
                }
                .help("Fetch every repository first, so your merged branches on the remote show up. Uses the network.")
                Button { Task { await model.scan() } } label: {
                    Label("Rescan", systemImage: "arrow.clockwise")
                }
                .keyboardShortcut("r")
                .help("Scan again")
            }
        }
        .disabled(model.busy)
        .fileImporter(isPresented: $choosingFolder, allowedContentTypes: [.folder]) { result in
            if case .success(let folder) = result { Task { await model.choose(folder) } }
        }
        .onChange(of: model.checkRemotes) { Task { await model.scan() } }
        .task { await model.scan() }
    }

    @ViewBuilder private var content: some View {
        switch model.phase {
        case .scanning(let entries):
            VStack(spacing: 6) {
                Text("Scanning \(tilde(model.root.path))").font(.title3)
                Text("\(entries.formatted()) items so far").monospacedDigit().foregroundStyle(.secondary)
            }
        case .failed(let message):
            ContentUnavailableView("Couldn't scan", systemImage: "exclamationmark.triangle", description: Text(message))
        case .reviewed(let review) where review.findings.isEmpty:
            ContentUnavailableView(
                "All fresh",
                systemImage: "sparkles",
                description: Text("Nothing in \(tilde(review.root)) is worth cleaning up.")
            )
        case .reviewed(let review):
            ReviewList(findings: review.findings)
        }
    }

    private var subtitle: String {
        guard let review = model.review else { return tilde(model.root.path) }
        return "\(tilde(review.root)) · \(review.bytes.formattedBytes) in \(review.files.formatted()) files"
    }
}
