import AppKit

/// A suggestion's look in the grid: an installed app, or a website.
struct GridItem {
    let id: String
    let name: String
    let detail: String
    let icon: Icon

    /// An app's icon, or a site's own once it has arrived (ADR-0019).
    enum Icon {
        case app(NSImage)
        case site(NSImage?)
    }

    /// A website by its name, with its domain under it.
    init(site name: String, host: String, icon: NSImage?) {
        id = host
        self.name = name
        detail = host
        self.icon = .site(icon)
    }

    /// The app with this bundle ID, by its installed name and icon, or nil when it isn't
    /// installed.
    init?(app bundleId: String, kind: String) {
        guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleId) else { return nil }
        id = bundleId
        name = Apps.name(at: url)
        detail = kind
        icon = .app(Apps.icon(at: url, side: 24))
    }
}
