import FreshCore
import SwiftUI

/// Sidebar on the window's gray, and the main column on a white card with a rounded
/// corner, under a slim strip for the window controls.
struct ContentView: View {
    static let sidebarWidth: CGFloat = 244

    @Environment(AppModel.self) private var model

    var body: some View {
        VStack(spacing: 0) {
            // An empty strip for the window controls; the sidebar says where you are.
            Color.clear.frame(height: 34)
            HStack(spacing: 0) {
                Sidebar()
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
        .onChange(of: model.checkRemotes) { Task { await model.scan() } }
        .task { await model.scan() }
    }
}
