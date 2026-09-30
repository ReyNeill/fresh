import SwiftUI

@main
struct FreshApp: App {
    @State private var model = AppModel()

    var body: some Scene {
        Window("Fresh", id: "main") {
            ContentView()
                .environment(model)
                .frame(minWidth: 760, minHeight: 480)
        }
        .windowToolbarStyle(.unified)
    }
}
