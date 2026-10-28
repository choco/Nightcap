import AppKit

/// The watch the core asks for (ADR-0012): the browser, found by its running process, whose host
/// `BrowserWatcher` reads on a thread of its own, and the host last read.
final class BrowserWatch: BrowserWatching {
    private(set) var host: String?
    private let onHost: (UInt64, String?) -> Void
    private var watcher: BrowserWatcher?

    /// `onHost` runs on the main thread with each host read under a generation, and at once with
    /// nil when the browser isn't running.
    init(onHost: @escaping (UInt64, String?) -> Void) {
        self.onHost = onHost
        watcher = BrowserWatcher { [weak self] generation, host in self?.read(generation, host) }
    }

    func watch(bundleId: String, generation: UInt64) {
        host = nil
        guard
            let pid = NSRunningApplication.runningApplications(withBundleIdentifier: bundleId)
                .first?.processIdentifier
        else {
            read(generation, nil)
            return
        }
        watcher?.watch(pid: pid, generation: generation)
    }

    func stop() {
        host = nil
        watcher?.stop()
    }

    private func read(_ generation: UInt64, _ host: String?) {
        self.host = host
        onHost(generation, host)
    }
}
