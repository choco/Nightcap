import AppKit
import os

/// Sites' own icons, for the menu's site row, the Rules tab and onboarding's suggestions
/// (ADR-0019). The menu and onboarding ask for them; each comes from its site and is kept on
/// disk, so a site is asked again only once its icon is a month old, or a day after it had none.
final class SiteIcons {
    /// Called with each site whose icon has just arrived.
    var onArrival: ((String) -> Void)?
    private let directory: URL
    /// False keeps to what's on disk and never asks a site, for the UI snapshots.
    private let online: Bool
    private var loaded: [String: NSImage] = [:]
    private var asking: Set<String> = []
    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "icons")

    private static let kept: TimeInterval = 30 * 24 * 60 * 60
    private static let missing: TimeInterval = 24 * 60 * 60
    /// The menu draws an icon at 16 points, so a declared icon of 32 pixels and up looks sharp.
    nonisolated private static let wanted = 32

    init(
        directory: URL = URL.cachesDirectory.appending(path: Bundle.main.bundleIdentifier!).appending(
            path: "Site Icons"),
        online: Bool = true
    ) {
        self.directory = directory
        self.online = online
    }

    /// The site's kept icon, if it has one.
    func icon(for site: String) -> NSImage? {
        if let image = loaded[site] { return image }
        guard Self.isPublic(site), let image = NSImage(contentsOf: file(site, "png")) else { return nil }
        loaded[site] = image
        return image
    }

    /// Asks each site for its icon, unless one is kept or it's too soon to ask again.
    func ask(_ sites: [String]) {
        guard online else { return }
        for site in Set(sites) where Self.isPublic(site) && !asking.contains(site) && due(site) {
            asking.insert(site)
            Task {
                let png = await Self.fetch(site)
                asking.remove(site)
                do {
                    try keep(png, for: site)
                } catch {
                    // The error's own description holds the file's path, which names the site.
                    let error = error as NSError
                    log.error("couldn't keep a site's icon: \(error.domain, privacy: .public) \(error.code)")
                }
                if png != nil { onArrival?(site) }
            }
        }
    }

    /// Writes the icon, or the marker that the site had none, which leaves an older icon shown.
    private func keep(_ png: Data?, for site: String) throws {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let marker = file(site, "none")
        guard let png else {
            try Data().write(to: marker)
            return
        }
        try png.write(to: file(site, "png"), options: .atomic)
        if FileManager.default.fileExists(atPath: marker.path) { try FileManager.default.removeItem(at: marker) }
        loaded[site] = NSImage(data: png)
    }

    private func due(_ site: String) -> Bool {
        let age = { (url: URL) in
            (try? url.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate
                .map { -$0.timeIntervalSinceNow }
        }
        if let age = age(file(site, "png")), age < Self.kept { return false }
        if let age = age(file(site, "none")), age < Self.missing { return false }
        return true
    }

    private func file(_ site: String, _ kind: String) -> URL {
        directory.appending(path: "\(site).\(kind)")
    }

    /// A site on the internet, by a name as the core reports one, which is also safe as a file
    /// name: not an address, nor a name only the local network knows, which macOS asks the user
    /// about before an app may reach it.
    nonisolated private static func isPublic(_ host: String) -> Bool {
        guard host.wholeMatch(of: /[a-z0-9-]+(\.[a-z0-9-]+)+/) != nil, host.wholeMatch(of: /[0-9.]+/) == nil,
            let last = host.split(separator: ".").last
        else { return false }
        return !["local", "lan", "home", "internal", "intranet", "corp", "localhost", "arpa"].contains(String(last))
    }

    /// Ephemeral, so no cookies go with a request or stay after it, and nothing's cached but
    /// the icons themselves.
    nonisolated private static let session: URLSession = {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.httpCookieAcceptPolicy = .never
        configuration.httpShouldSetCookies = false
        configuration.urlCache = nil
        configuration.timeoutIntervalForRequest = 10
        configuration.timeoutIntervalForResource = 20
        return URLSession(configuration: configuration, delegate: PublicRedirects(), delegateQueue: nil)
    }()

    /// Follows a redirect only to another site on the internet.
    nonisolated private final class PublicRedirects: NSObject, URLSessionTaskDelegate, Sendable {
        func urlSession(
            _ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
            newRequest request: URLRequest, completionHandler: @escaping @Sendable (URLRequest?) -> Void
        ) {
            let host = request.url?.host()?.lowercased()
            completionHandler(host.map(SiteIcons.isPublic) == true ? request : nil)
        }
    }

    /// The site's icon as a PNG: the best its home page declares, then /favicon.ico.
    @concurrent nonisolated private static func fetch(_ site: String) async -> Data? {
        guard let home = URL(string: "https://\(site)/") else { return nil }
        var candidates: [URL] = []
        // A home page that doesn't load or parse still leaves /favicon.ico to try.
        if let (page, response) = try? await session.data(from: home), let url = response.url {
            candidates = declaredIcons(in: page, at: url)
        }
        candidates.append(home.appending(path: "favicon.ico"))
        for url in candidates {
            guard let (data, response) = try? await session.data(from: url),
                (response as? HTTPURLResponse)?.statusCode == 200,
                let image = NSImage(data: data), image.isValid,
                let png = png(image)
            else { continue }
            return png
        }
        return nil
    }

    /// The icons a page declares, best first: its own icons before the touch icon, and of each,
    /// the nearest at or above 32 pixels, an SVG, one of no stated size, then smaller ones.
    nonisolated private static func declaredIcons(in page: Data, at url: URL) -> [URL] {
        guard let document = try? XMLDocument(data: page, options: [.documentTidyHTML]) else { return [] }
        let base =
            (try? document.nodes(forXPath: "//base/@href"))?.first?.stringValue
            .flatMap { URL(string: $0, relativeTo: url) } ?? url
        let links = (try? document.nodes(forXPath: "//link[@rel][@href]")) ?? []
        let ranked = links.compactMap { node -> (url: URL, rank: (Int, Int))? in
            guard let link = node as? XMLElement,
                let rel = link.attribute(forName: "rel")?.stringValue?.lowercased().split(separator: " "),
                let href = link.attribute(forName: "href")?.stringValue,
                let target = URL(string: href, relativeTo: base)?.absoluteURL, target.scheme == "https",
                let host = target.host()?.lowercased(), isPublic(host)
            else { return nil }
            let touch = rel.contains("apple-touch-icon") || rel.contains("apple-touch-icon-precomposed")
            guard touch || rel.contains("icon") else { return nil }
            let sizes = link.attribute(forName: "sizes")?.stringValue?.lowercased() ?? ""
            let size = sizes.split(separator: " ").compactMap { Int($0.split(separator: "x").first ?? "") }.max()
            let svg =
                sizes == "any" || link.attribute(forName: "type")?.stringValue == "image/svg+xml"
                || target.pathExtension.lowercased() == "svg"
            let fit =
                if let size, size >= wanted { size } else if svg { 10_000 } else if let size { 30_000 - size } else {
                    20_000
                }
            return (target, (touch ? 1 : 0, fit))
        }
        return ranked.sorted { $0.rank < $1.rank }.map(\.url)
    }

    /// The image fitted into a square of 32 points at 64 pixels.
    nonisolated private static func png(_ image: NSImage) -> Data? {
        let side: CGFloat = 32
        guard
            let bitmap = NSBitmapImageRep(
                bitmapDataPlanes: nil, pixelsWide: Int(side) * 2, pixelsHigh: Int(side) * 2, bitsPerSample: 8,
                samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0,
                bitsPerPixel: 0)
        else { return nil }
        bitmap.size = NSSize(width: side, height: side)
        let scale = side / max(image.size.width, image.size.height)
        let size = NSSize(width: image.size.width * scale, height: image.size.height * scale)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
        image.draw(
            in: NSRect(
                x: (side - size.width) / 2, y: (side - size.height) / 2, width: size.width, height: size.height))
        NSGraphicsContext.restoreGraphicsState()
        return bitmap.representation(using: .png, properties: [:])
    }
}
