import Foundation
import NightcapCore

/// What a notification says: about something the core told, an outside change adopted as a
/// snooze or a command the core refused, which left what the user chose undone; or about a
/// problem that appeared on its own, or the first launch after an update, which the app tells.
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

    /// The notice for `event`, in `words`, or nil where the user isn't told: an adoption that
    /// snoozed no rule.
    init?(_ event: Event, frontApp: FrontApp?, words: Wording) {
        // A rule goes by its app's name while that app is in front, else by its bundle ID.
        let rule = { (identity: RuleIdentity) -> Wording.Place in
            switch identity {
            case .app(let bundleId):
                .app(
                    name: frontApp.flatMap { app in
                        app.bundleId.caseInsensitiveCompare(bundleId) == .orderedSame ? app.name : nil
                    } ?? bundleId)
            case .domain(let domain): .site(domain: domain)
            }
        }
        switch event {
        case .adopted(_, let snoozed, let state, let until):
            guard let snoozed else { return nil }
            self.init(
                title: words.turnedOutsideNightcap(state), body: words.snoozed(rule(snoozed), state, until: until))
        case .refused(_, let why):
            self.init(title: words.refused, body: words.refusal(why, rule: rule))
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
