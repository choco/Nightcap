import AppKit
import NightcapCore

/// Apps on this Mac, as Settings and onboarding name them and let the user pick one.
enum Apps {
    /// The name Finder shows for the app at `url`, without ".app" unless Finder shows every
    /// extension.
    static func name(at url: URL) -> String {
        FileManager.default.displayName(atPath: url.path)
    }

    /// A sheet over the Applications folder, which hands `chosen` the bundle ID of the app picked,
    /// unless it has none a rule can name.
    static func pick(
        on window: NSWindow, prompt: String, message: String, accessory: NSView? = nil,
        chosen: @escaping (_ bundleId: String) -> Void
    ) {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.application]
        panel.directoryURL = URL(fileURLWithPath: "/Applications")
        panel.prompt = prompt
        panel.message = message
        if let accessory {
            panel.accessoryView = accessory
            panel.isAccessoryViewDisclosed = true
        }
        panel.beginSheetModal(for: window) { response in
            guard response == .OK, let url = panel.url, let bundleId = Bundle(url: url)?.bundleIdentifier,
                isBundleId(text: bundleId)
            else { return }
            chosen(bundleId)
        }
    }
}
