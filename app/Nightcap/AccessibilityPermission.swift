import AppKit
import ApplicationServices
import notify

/// The Accessibility grant, kept current without a relaunch (ADR-0012).
///
/// `tccd` posts the Darwin notification `com.apple.tcc.access.changed` right after each write to
/// its database, for any app, in either direction. TCC's private `TCCAccessCheckAuditToken` reads
/// the result from `tccd`, already correct when the notification arrives. `AXIsProcessTrusted()`
/// answers from an in-process cache refreshed by `com.apple.accessibility.api`, which System
/// Settings posts before `tccd` writes, so it can keep the old answer; it's only the fallback.
final class AccessibilityPermission {
    private(set) var granted: Bool
    private let onChange: (Bool) -> Void
    private var token = NOTIFY_TOKEN_INVALID

    /// `onChange` runs on the main thread whenever the grant flips.
    init(onChange: @escaping (Bool) -> Void) {
        granted = Self.check()
        self.onChange = onChange
        notify_register_dispatch("com.apple.tcc.access.changed", &token, .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.refresh() }
        }
    }

    /// The macOS prompt while Nightcap has no entry in the Accessibility list. Once it has one,
    /// switched on or off, the prompt never appears, so the pane in System Settings opens instead.
    func ask() {
        if Self.hasNoEntry() == true {
            _ = AXIsProcessTrustedWithOptions(["AXTrustedCheckOptionPrompt": true] as CFDictionary)
        } else {
            let pane = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
            NSWorkspace.shared.open(URL(string: pane)!)
        }
    }

    private func refresh() {
        let now = Self.check()
        guard now != granted else { return }
        granted = now
        onChange(now)
    }

    private static func check() -> Bool {
        guard let symbol = tccSymbol("TCCAccessCheckAuditToken") else { return AXIsProcessTrusted() }
        typealias Check = @convention(c) (CFString, audit_token_t, CFDictionary?) -> Bool
        return unsafeBitCast(symbol, to: Check.self)("kTCCServiceAccessibility" as CFString, ownAuditToken, nil)
    }

    /// TCC's private `TCCAccessPreflight` answers 2 while the app has no entry for the service,
    /// and something else once it has one. Nil when the function is missing.
    private static func hasNoEntry() -> Bool? {
        guard let symbol = tccSymbol("TCCAccessPreflight") else { return nil }
        typealias Preflight = @convention(c) (CFString, CFDictionary?) -> Int32
        return unsafeBitCast(symbol, to: Preflight.self)("kTCCServiceAccessibility" as CFString, nil) == 2
    }

    private static func tccSymbol(_ name: String) -> UnsafeMutableRawPointer? {
        guard let tcc = dlopen("/System/Library/PrivateFrameworks/TCC.framework/TCC", RTLD_LAZY) else { return nil }
        return dlsym(tcc, name)
    }

    private static let ownAuditToken: audit_token_t = {
        var token = audit_token_t()
        var count = mach_msg_type_number_t(MemoryLayout<audit_token_t>.size / MemoryLayout<natural_t>.size)
        let result = withUnsafeMutablePointer(to: &token) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                task_info(mach_task_self_, task_flavor_t(TASK_AUDIT_TOKEN), $0, &count)
            }
        }
        precondition(result == KERN_SUCCESS, "task_info couldn't read Nightcap's own audit token")
        return token
    }()
}
