import AppKit
import ApplicationServices
import os

/// Reads the frontmost browser's host through Accessibility, on a thread of its own, so a hung
/// browser never stalls the main thread (ADR-0010). Only hosts leave it, and it never logs one
/// (ADR-0012).
///
/// Everything below `runLoop` is confined to the watcher's thread: the main thread reaches it
/// only through `perform`.
nonisolated final class BrowserWatcher: @unchecked Sendable {
    /// Browsers that keep the page's URL on the first element under the focused window carrying
    /// `AXURL`, skipping links: Safari on its `AXWebArea`, Chrome on the window's top `AXGroup`.
    static let browsers: Set<String> = ["com.apple.Safari", "com.google.Chrome"]

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

    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "browser")
    private let onHost: @Sendable (UInt64, String?) -> Void
    private let onAccessibility: @Sendable (Bool) -> Void
    private var runLoop: CFRunLoop?

    private struct Watch {
        let pid: pid_t
        let generation: UInt64
        /// Observers deliver only while they're retained.
        let observer: AXObserver
        var readPending = false
        /// What was last reported, so an unchanged host isn't sent again.
        var reported: String??
    }

    private var watch: Watch?

    /// Both run on the main thread: `onHost` with each new host read under a generation, and
    /// `onAccessibility` with whether attaching to a browser showed Accessibility granted, which
    /// `AXIsProcessTrusted()` only learns at the next launch (M2).
    init(
        onHost: @escaping @MainActor (UInt64, String?) -> Void,
        onAccessibility: @escaping @MainActor (Bool) -> Void
    ) {
        self.onHost = { generation, host in
            DispatchQueue.main.async { MainActor.assumeIsolated { onHost(generation, host) } }
        }
        self.onAccessibility = { granted in
            DispatchQueue.main.async { MainActor.assumeIsolated { onAccessibility(granted) } }
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
            var observer: AXObserver?
            guard AXObserverCreate(pid, Self.observerCallback, &observer) == .success, let observer else {
                log.error("couldn't observe the browser; Accessibility may not be granted")
                report(generation, nil, force: true)
                return
            }
            let app = AXUIElementCreateApplication(pid)
            let refcon = Unmanaged.passUnretained(self).toOpaque()
            var granted = true
            for notification in Self.notifications {
                let error = AXObserverAddNotification(observer, app, notification as CFString, refcon)
                if error == .apiDisabled { granted = false }
                if error != .success {
                    log.error("couldn't observe \(notification, privacy: .public): \(error.rawValue)")
                }
            }
            onAccessibility(granted)
            CFRunLoopAddSource(CFRunLoopGetCurrent(), AXObserverGetRunLoopSource(observer), .defaultMode)
            watch = Watch(pid: pid, generation: generation, observer: observer)
            scheduleRead()
        }
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
            report(generation, Self.host(pid: current.pid), force: current.reported == nil)
        }
        RunLoop.current.add(timer, forMode: .default)
    }

    private func report(_ generation: UInt64, _ host: String?, force: Bool) {
        if !force, let current = watch, current.reported == .some(host) { return }
        watch?.reported = .some(host)
        onHost(generation, host)
    }

    /// The focused window's page host, searched breadth-first within the read budget.
    private static func host(pid: pid_t) -> String? {
        let deadline = Date().addingTimeInterval(readBudget)
        let app = AXUIElementCreateApplication(pid)
        AXUIElementSetMessagingTimeout(app, elementTimeout)
        guard let window = element(attribute(app, kAXFocusedWindowAttribute)) else { return nil }
        var queue: [(element: AXUIElement, depth: Int)] = [(window, 0)]
        var visited = 0
        while !queue.isEmpty, visited < maxVisited, Date() < deadline {
            let (element, depth) = queue.removeFirst()
            visited += 1
            AXUIElementSetMessagingTimeout(element, elementTimeout)
            let role = attribute(element, kAXRoleAttribute) as? String ?? ""
            if depth > 0, role != "AXLink", let url = attribute(element, "AXURL") {
                return (url as? URL)?.host() ?? (url as? String).flatMap { URL(string: $0)?.host() }
            }
            guard depth < maxDepth, !leafRoles.contains(role) else { continue }
            let children = attribute(element, kAXChildrenAttribute) as? [AnyObject] ?? []
            queue += children.compactMap(Self.element).map { ($0, depth + 1) }
        }
        return nil
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
