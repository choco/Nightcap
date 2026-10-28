import NightcapCore
import ServiceManagement

/// The login item that opens Nightcap at login, through `SMAppService` (ADR-0011).
final class LoginItemAdapter: LoginItemRegistering {
    /// Where the login item stands, which the user can change in System Settings. A
    /// never-registered app reports .notFound, the same as one whose item was removed.
    var item: LoginItem {
        switch SMAppService.mainApp.status {
        case .enabled: .enabled
        case .requiresApproval: .requiresApproval
        default: .notRegistered
        }
    }

    func register() throws {
        try SMAppService.mainApp.register()
    }

    func unregister() throws {
        try SMAppService.mainApp.unregister()
    }
}
