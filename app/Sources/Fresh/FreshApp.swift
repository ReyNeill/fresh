import SwiftUI

@main
struct FreshApp: App {
    @State private var model = AppModel()
    @State private var updates = Updates()

    var body: some Scene {
        Window("Fresh", id: "main") {
            ContentView()
                .environment(model)
                .environment(updates)
                .frame(minWidth: 880, minHeight: 540)
        }
        .windowStyle(.hiddenTitleBar)
        .windowBackgroundDragBehavior(.enabled)
        .defaultSize(width: 1160, height: 760)
        .commands {
            CommandGroup(after: .appInfo) {
                Button("Check for Updates…") { updates.check() }
            }
            // There's no text to edit, so Edit ▸ Undo means the last clean-up.
            CommandGroup(replacing: .undoRedo) {
                Button("Undo Last Clean-Up") { Task { await model.undo() } }
                    .keyboardShortcut("z")
                    .disabled(model.busy)
            }
            CommandGroup(after: .newItem) {
                Button("Scan Again") { Task { await model.scan() } }
                    .keyboardShortcut("r")
                    .disabled(model.isScanning || model.busy)
            }
        }

        Settings {
            SettingsView().environment(model)
        }
    }
}
