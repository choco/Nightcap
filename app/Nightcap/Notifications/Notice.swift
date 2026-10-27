import Foundation
import NightcapCore

/// What a notification says about something the core told: an external change adopted as a
/// snooze, the first launch after an update, or a problem that appeared on its own.
struct Notice: Equatable {
    let title: String
    let body: String
    /// The page a click on the notification opens.
    var opens: URL?

    /// The app in front, which a snoozed rule of its own is named by.
    struct FrontApp {
        let bundleId: String
        let name: String?
    }

    /// The notice for `event`, in `words`, or nil where the user isn't told: an adoption that
    /// snoozed no rule, or a refused command, which the user is looking at already.
    init?(_ event: Event, frontApp: FrontApp?, words: Wording) {
        switch event {
        case .adopted(_, let snoozed, let state, let until):
            guard let snoozed else { return nil }
            let rule: Wording.Place =
                switch snoozed {
                case .app(let bundleId):
                    .app(
                        name: frontApp.flatMap { app in
                            app.bundleId.caseInsensitiveCompare(bundleId) == .orderedSame ? app.name : nil
                        } ?? bundleId)
                case .domain(let domain): .site(domain: domain)
                }
            self.init(title: words.turnedOutsideNightcap(state), body: words.snoozed(rule, state, until: until))
        case .updated(_, let to):
            self.init(
                title: words.updated(to: to), body: words.whatsNew,
                opens: URL(string: "https://nightcap.choco.me/changelog/#v\(to)"))
        case .problemAppeared(let check):
            self.init(problem: check.detail, words: words)
        case .refused:
            return nil
        }
    }

    init(title: String, body: String, opens: URL? = nil) {
        self.title = title
        self.body = body
        self.opens = opens
    }

    /// A problem as the core announces it: never Night Shift unavailable, notifications switched
    /// off or a login item waiting for approval, nor a check that's no problem.
    private init?(problem detail: CheckDetail, words: Wording) {
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
