import NightcapCore
import ServiceManagement
import os

/// The login item that opens Nightcap at login, through `SMAppService` (ADR-0011). Each change
/// reports the status macOS actually left, including when the attempt failed.
final class LoginItemAdapter: LoginItemRegistering {
    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "login-item")
    private let read: () -> LoginItem
    private let registerItem: () throws -> Void
    private let unregisterItem: () throws -> Void
    private let onChange: (LoginItem) -> Void

    convenience init(onChange: @escaping (LoginItem) -> Void) {
        self.init(
            read: {
                switch SMAppService.mainApp.status {
                case .enabled: .enabled
                case .requiresApproval: .requiresApproval
                default: .notRegistered
                }
            },
            register: { try SMAppService.mainApp.register() },
            unregister: { try SMAppService.mainApp.unregister() },
            onChange: onChange)
    }

    /// The operations are supplied together so tests can exercise failed attempts without
    /// changing this Mac's login item.
    init(
        read: @escaping () -> LoginItem,
        register: @escaping () throws -> Void,
        unregister: @escaping () throws -> Void,
        onChange: @escaping (LoginItem) -> Void
    ) {
        self.read = read
        registerItem = register
        unregisterItem = unregister
        self.onChange = onChange
    }

    /// Where the login item stands, which the user can change in System Settings.
    var item: LoginItem { read() }

    func register() { change(registerItem) }
    func unregister() { change(unregisterItem) }

    private func change(_ operation: () throws -> Void) {
        do {
            try operation()
        } catch {
            log.error("couldn't change the login item: \(error, privacy: .public)")
        }
        onChange(item)
    }
}
