import AppKit
import ApplicationServices
import os

/// Reads the frontmost browser's host through Accessibility, on a thread of its own, so a hung
/// browser never stalls the main thread (ADR-0012). Only hosts leave it, and it never logs one
/// (ADR-0009).
///
/// Everything below `runLoop` is confined to the watcher's thread: the main thread reaches it
/// only through `perform`.
nonisolated final class BrowserWatcher: @unchecked Sendable {
    /// Browsers that keep the page's URL on the first element under the focused window carrying
    /// `AXURL`, skipping links: Safari on its `AXWebArea`, Chrome on the window's top `AXGroup`.
    /// Opera and Vivaldi keep the page out of Accessibility's reach (ADR-0012).
    static let browsers: Set<String> = [
        "com.apple.Safari", "com.apple.SafariTechnologyPreview", "com.kagi.kagimacOS",
        "com.google.Chrome", "com.microsoft.edgemac", "com.brave.Browser",
        "company.thebrowser.Browser", "company.thebrowser.dia",
        "org.mozilla.firefox", "app.zen-browser.zen",
    ]

    /// Every tab switch comes with a focus change. A page loading in the focused tab, such as a
    /// link opening Safari, changes only the title. `AXValueChanged` is mostly noise (M0).
    private static let notifications = [
        kAXFocusedWindowChangedNotification, kAXFocusedUIElementChangedNotification,
        kAXTitleChangedNotification,
    ]
    /// Branches that never hold page content: the toolbar, the tab strip and plain controls.
    private static let leafRoles: Set<String> = [
        "AXToolbar", "AXOpaqueProviderGroup", "AXButton", "AXRadioButton", "AXMenuButton",
        "AXTextField", "AXStaticText", "AXImage", "AXSplitter", "AXCheckBox", "AXPopUpButton",
    ]
    /// One change fires a burst of notifications; they're merged into one read.
    private static let mergeDelay: TimeInterval = 0.05
    private static let elementTimeout: Float = 0.1
    private static let readBudget: TimeInterval = 0.3
    private static let maxVisited = 200
    private static let maxDepth = 15
    /// A browser that's still launching refuses observers until its Accessibility is up.
    private static let observeRetryInterval: TimeInterval = 0.25
    private static let observeRetries = 40
    /// A page can be missing from the tree for a moment, while Firefox builds its tree or a page
    /// loads, so a read that finds none is tried again, keeping the last host until the retries
    /// run out.
    private static let missingRetryInterval: TimeInterval = 0.25
    private static let missingRetries = 8

    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "browser")
    private let onHost: @Sendable (UInt64, String?) -> Void
    private var runLoop: CFRunLoop?

    private struct Watch {
        let pid: pid_t
        let generation: UInt64
        /// Observers deliver only while they're retained.
        let observer: AXObserver
        var readPending = false
        /// What was last reported, so an unchanged host isn't sent again.
        var reported: String??
        var missingRetriesLeft = BrowserWatcher.missingRetries
    }

    /// What a read found: a page, with its host if it's a web page, or none.
    private enum Read {
        case page(host: String?)
        case missing
    }

    private var watch: Watch?

    /// `onHost` runs on the main thread with each new host read under a generation.
    init(onHost: @escaping @MainActor (UInt64, String?) -> Void) {
        self.onHost = { generation, host in
            DispatchQueue.main.async { MainActor.assumeIsolated { onHost(generation, host) } }
        }
        let started = DispatchSemaphore(value: 0)
        let thread = Thread { [unowned self] in
            runLoop = CFRunLoopGetCurrent()
            started.signal()
            // A run loop with no sources returns at once; this port keeps it alive.
            RunLoop.current.add(NSMachPort(), forMode: .default)
            RunLoop.current.run()
        }
        thread.name = "Nightcap browser watcher"
        thread.start()
        started.wait()
    }

    /// Replaces any earlier watch. The first read is always reported, unreadable as nil.
    func watch(pid: pid_t, generation: UInt64) {
        perform { [self] in
            stopWatching()
            // Firefox and its forks build their tree only once a client asks for the app's role.
            let app = AXUIElementCreateApplication(pid)
            AXUIElementSetMessagingTimeout(app, Self.elementTimeout)
            _ = Self.attribute(app, kAXRoleAttribute)
            var observer: AXObserver?
            guard AXObserverCreate(pid, Self.observerCallback, &observer) == .success, let observer else {
                log.error("couldn't observe the browser; Accessibility may not be granted")
                report(generation, nil, force: true)
                return
            }
            CFRunLoopAddSource(CFRunLoopGetCurrent(), AXObserverGetRunLoopSource(observer), .defaultMode)
            watch = Watch(pid: pid, generation: generation, observer: observer)
            observe(Self.notifications, generation: generation, retriesLeft: Self.observeRetries)
            scheduleRead()
        }
    }

    /// Adds `notifications` to the current watch. The ones a launching browser can't take yet
    /// are retried, and once they're all in place the page is read again: until then, nothing
    /// would announce it.
    private func observe(_ notifications: [String], generation: UInt64, retriesLeft: Int) {
        guard let current = watch, current.generation == generation else { return }
        let app = AXUIElementCreateApplication(current.pid)
        let refcon = Unmanaged.passUnretained(self).toOpaque()
        var refused: [String] = []
        for notification in notifications {
            let error = AXObserverAddNotification(current.observer, app, notification as CFString, refcon)
            if error == .cannotComplete, retriesLeft > 0 {
                refused.append(notification)
            } else if error != .success {
                log.error("couldn't observe \(notification, privacy: .public): \(error.rawValue)")
            }
        }
        guard !refused.isEmpty else {
            if retriesLeft < Self.observeRetries { scheduleRead() }
            return
        }
        let timer = Timer(timeInterval: Self.observeRetryInterval, repeats: false) { [self, refused] _ in
            observe(refused, generation: generation, retriesLeft: retriesLeft - 1)
        }
        RunLoop.current.add(timer, forMode: .default)
    }

    func stop() {
        perform { [self] in stopWatching() }
    }

    private func perform(_ block: @escaping () -> Void) {
        CFRunLoopPerformBlock(runLoop, CFRunLoopMode.defaultMode.rawValue, block)
        CFRunLoopWakeUp(runLoop)
    }

    private static let observerCallback: AXObserverCallback = { _, _, _, refcon in
        guard let refcon else { return }
        Unmanaged<BrowserWatcher>.fromOpaque(refcon).takeUnretainedValue().scheduleRead()
    }

    private func stopWatching() {
        guard let watch else { return }
        CFRunLoopRemoveSource(CFRunLoopGetCurrent(), AXObserverGetRunLoopSource(watch.observer), .defaultMode)
        self.watch = nil
    }

    private func scheduleRead() {
        guard let current = watch, !current.readPending else { return }
        watch?.readPending = true
        let generation = current.generation
        let timer = Timer(timeInterval: Self.mergeDelay, repeats: false) { [self] _ in
            guard let current = watch, current.generation == generation else { return }
            watch?.readPending = false
            switch Self.read(pid: current.pid) {
            case .page(let host):
                watch?.missingRetriesLeft = Self.missingRetries
                report(generation, host, force: current.reported == nil)
            case .missing:
                if current.reported == nil || current.missingRetriesLeft == 0 {
                    report(generation, nil, force: current.reported == nil)
                }
                retryMissing(generation)
            }
        }
        RunLoop.current.add(timer, forMode: .default)
    }

    private func retryMissing(_ generation: UInt64) {
        guard let current = watch, current.generation == generation, current.missingRetriesLeft > 0 else { return }
        watch?.missingRetriesLeft -= 1
        let timer = Timer(timeInterval: Self.missingRetryInterval, repeats: false) { [self] _ in
            guard watch?.generation == generation else { return }
            scheduleRead()
        }
        RunLoop.current.add(timer, forMode: .default)
    }

    private func report(_ generation: UInt64, _ host: String?, force: Bool) {
        if !force, let current = watch, current.reported == .some(host) { return }
        watch?.reported = .some(host)
        onHost(generation, host)
    }

    /// The focused window's page, searched breadth-first within the read budget.
    private static func read(pid: pid_t) -> Read {
        let deadline = Date().addingTimeInterval(readBudget)
        let app = AXUIElementCreateApplication(pid)
        AXUIElementSetMessagingTimeout(app, elementTimeout)
        guard let window = element(attribute(app, kAXFocusedWindowAttribute)) else { return .missing }
        var queue: [(element: AXUIElement, depth: Int)] = [(window, 0)]
        var visited = 0
        while !queue.isEmpty, visited < maxVisited, Date() < deadline {
            let (element, depth) = queue.removeFirst()
            visited += 1
            AXUIElementSetMessagingTimeout(element, elementTimeout)
            let role = attribute(element, kAXRoleAttribute) as? String ?? ""
            if depth > 0, role != "AXLink", let value = attribute(element, "AXURL") {
                // Only web pages have a host a domain rule means; chrome://newtab doesn't.
                let url = value as? URL ?? (value as? String).flatMap { URL(string: $0) }
                guard let url, ["http", "https"].contains(url.scheme?.lowercased()) else { return .page(host: nil) }
                return .page(host: url.host())
            }
            guard depth < maxDepth, !leafRoles.contains(role) else { continue }
            let children = attribute(element, kAXChildrenAttribute) as? [AnyObject] ?? []
            queue += children.compactMap(Self.element).map { ($0, depth + 1) }
        }
        return .missing
    }

    private static func attribute(_ element: AXUIElement, _ name: String) -> AnyObject? {
        var value: AnyObject?
        return AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success ? value : nil
    }

    private static func element(_ value: AnyObject?) -> AXUIElement? {
        guard let value, CFGetTypeID(value) == AXUIElementGetTypeID() else { return nil }
        return unsafeDowncast(value, to: AXUIElement.self)
    }
}
