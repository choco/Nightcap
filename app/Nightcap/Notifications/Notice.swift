import Foundation
import NightcapCore

/// What a notification says: about an outside change the core adopted as a snooze, or a command
/// of the app's the core refused, which left what the user chose undone; or about a problem that
/// appeared on its own, or the first launch after an update, which the app tells.
struct Notice: Equatable {
    let title: String
    let body: String
    /// The page a click on the notification opens.
    var opens: URL?

    /// The app in front, which a rule of its own is named by.
    struct FrontApp {
        let bundleId: String
        let name: String?
    }

    /// The notice for an outside change taken as the user's, in `words`, or nil where it snoozed
    /// no rule.
    init?(adoptedSnoozing snoozed: RuleIdentity?, state: OnOff, until: Date?, frontApp: FrontApp?, words: Wording) {
        guard let snoozed else { return nil }
        self.init(
            title: words.turnedOutsideNightcap(state),
            body: words.snoozed(Self.rule(snoozed, frontApp: frontApp), state, until: until))
    }

    /// The notice for a command of the app's the core refused, in `words`, or nil for a reload,
    /// whose refusal the config file's check shows already.
    init?(refused command: Command, why: Refusal, frontApp: FrontApp?, words: Wording) {
        if case .reload = command { return nil }
        self.init(title: words.refused, body: words.refusal(why) { Self.rule($0, frontApp: frontApp) })
    }

    /// A rule goes by its app's name while that app is in front, else by its bundle ID.
    private static func rule(_ identity: RuleIdentity, frontApp: FrontApp?) -> Wording.Place {
        switch identity {
        case .app(let bundleId):
            .app(
                name: frontApp.flatMap { app in
                    app.bundleId.caseInsensitiveCompare(bundleId) == .orderedSame ? app.name : nil
                } ?? bundleId)
        case .domain(let domain): .site(domain: domain)
        }
    }

    init(title: String, body: String, opens: URL? = nil) {
        self.title = title
        self.body = body
        self.opens = opens
    }

    /// The first launch of `version` after an update, which opens its changes in the changelog.
    init(updatedTo version: String, words: Wording) {
        self.init(
            title: words.updated(to: version), body: words.whatsNew,
            opens: URL(string: "https://nightcap.choco.me/changelog/#v\(version)"))
    }

    /// A problem as the app announces it, or nil for one it never does, nor a check that's no
    /// problem: the one filter of what can be told. Night Shift being unavailable is a fact about
    /// the Mac, switched-off notifications can't announce themselves, and a login item waiting
    /// for approval follows the user's own choice, which macOS announces itself.
    init?(problem detail: CheckDetail, words: Wording) {
        let notice: (title: String, body: String)
        switch detail {
        case .nightShift(.setAside): notice = words.scheduleSetAside
        case .nightShift(.noSunTimes): notice = words.noSunTimes
        case .accessibility(.notAllowed): notice = words.accessibilityNotAllowed
        case .openAtLogin(.removed): notice = words.openAtLoginRemoved
        case .configFile(.error(let error)): notice = words.configInvalid(error)
        case .adHocRules(.off(let error)): notice = words.adHocRulesOff(error)
        case .nightShift(.available), .nightShift(.unavailable), .accessibility(.allowed),
            .accessibility(.notNeeded), .notifications, .openAtLogin(.opens), .openAtLogin(.awaitingApproval),
            .openAtLogin(.off), .configFile(.valid), .configFile(.missing), .adHocRules(.on):
            return nil
        }
        self.init(title: notice.title, body: notice.body)
    }
}
