import FreshCore
import SwiftUI

/// Sidebar on the window's gray, and the main column on a white card with a rounded
/// corner, under a slim title bar shared with the window controls.
struct ContentView: View {
    static let sidebarWidth: CGFloat = 244

    @Environment(AppModel.self) private var model
    @State private var choosingFolder = false

    var body: some View {
        VStack(spacing: 0) {
            TitleBar()
            HStack(spacing: 0) {
                Sidebar(chooseFolder: { choosingFolder = true })
                    .frame(width: Self.sidebarWidth)
                MainPanel()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(Palette.surface, in: UnevenRoundedRectangle(topLeadingRadius: Radius.card))
                    .overlay(UnevenRoundedRectangle(topLeadingRadius: Radius.card).strokeBorder(Palette.border))
            }
        }
        .background(Palette.window)
        .ignoresSafeArea()
        .foregroundStyle(Palette.text)
        .fileImporter(isPresented: $choosingFolder, allowedContentTypes: [.folder]) { result in
            if case .success(let folder) = result { Task { await model.choose(folder) } }
        }
        .onChange(of: model.checkRemotes) { Task { await model.scan() } }
        .task { await model.scan() }
    }
}

/// The window controls' row: the shown group's title sits above the main column.
private struct TitleBar: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        HStack(spacing: 7) {
            Image(systemName: model.showingMap ? "square.grid.3x2" : model.group?.symbol ?? "tray.full")
                .font(.system(size: 12))
                .foregroundStyle(Palette.secondaryText)
            Text(model.showingMap ? "Space map" : model.group?.title ?? "Everything")
                .font(.system(size: 13, weight: .medium))
            Text(tilde(model.root.path))
                .font(.system(size: 13))
                .foregroundStyle(Palette.tertiaryText)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer()
        }
        .padding(.leading, ContentView.sidebarWidth + 16)
        .padding(.trailing, 16)
        // Centered on the traffic lights, which sit 14pt from the top of the window.
        .padding(.bottom, 6)
        .frame(height: 34)
    }
}
