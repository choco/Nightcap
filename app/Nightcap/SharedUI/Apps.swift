import AppKit
import NightcapCore

/// Apps on this Mac, as Settings, onboarding and the tour name them and draw their icons, and let
/// the user pick one.
enum Apps {
    /// The name Finder shows for the app at `url`, without ".app" unless Finder shows every
    /// extension.
    static func name(at url: URL) -> String {
        FileManager.default.displayName(atPath: url.path)
    }

    /// The icon of the app at `url`, drawn once at `side` points on a Retina screen, since icons
    /// from an asset catalog fill in lazily, blank until something asks for their pixels.
    static func icon(at url: URL, side: CGFloat) -> NSImage {
        let icon = NSWorkspace.shared.icon(forFile: url.path)
        var rect = NSRect(x: 0, y: 0, width: side * 2, height: side * 2)
        guard let pixels = icon.cgImage(forProposedRect: &rect, context: nil, hints: nil) else { return icon }
        return NSImage(cgImage: pixels, size: NSSize(width: side, height: side))
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
