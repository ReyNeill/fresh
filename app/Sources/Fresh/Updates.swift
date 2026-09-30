import Sparkle
import SwiftUI

/// In-app updates through Sparkle: a daily check (`SUEnableAutomaticChecks` in Info.plist)
/// and "Check for Updates…". Updates come from the appcast on the latest GitHub release, and
/// Sparkle only installs ones signed with the release key and the app's own certificate.
@MainActor @Observable
final class Updates {
    @ObservationIgnored private let controller = SPUStandardUpdaterController(
        startingUpdater: true,
        updaterDelegate: nil,
        userDriverDelegate: nil
    )

    func check() {
        controller.checkForUpdates(nil)
    }
}
