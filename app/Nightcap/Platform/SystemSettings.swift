import AppKit
import ServiceManagement

/// Opens the panes of System Settings where the user puts back what they set up.
struct SystemSettings: SystemSettingsOpening {
    func open(_ pane: SystemSettingsPane) {
        switch pane {
        case .loginItems:
            SMAppService.openSystemSettingsLoginItems()
        case .locationServices:
            let url = "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_LocationServices"
            NSWorkspace.shared.open(URL(string: url)!)
        case .notifications:
            NSWorkspace.shared.open(
                URL(
                    string: "x-apple.systempreferences:com.apple.Notifications-Settings.extension"
                        + "?id=\(Bundle.main.bundleIdentifier!)")!)
        }
    }
}
