import Foundation
import NightcapCore

/// What the settings window shows, decided from the core's queries and worded by `Wording`: the
/// switches and whether the config locks them, the checks under their settings with their fixes,
/// where the config is read from, the Rules tab's rules and what's on or off for now. The window
/// draws it and sends back the commands and fixes it carries.
struct Settings: Equatable {
    enum Tab: String, CaseIterable {
        case general, rules

        /// The kinds of check the tab shows, each under the setting it belongs to.
        var checks: [CheckKind] {
            switch self {
            case .general: [.nightShift, .accessibility, .notifications, .openAtLogin]
            case .rules: [.configFile, .adHocRules]
            }
        }
    }

    /// Flipping it sends `command`.
    struct Switch: Equatable {
        let on: Bool
        /// Set by the config, so Settings can't change it.
        let locked: Bool
        let command: Command
    }

    /// A button that sends `command`.
    struct Action: Equatable {
        let title: String
        let command: Command
    }

    /// A button that carries out `fix`, the shell's own or the core's.
    struct FixButton: Equatable {
        let title: String
        let fix: Fix
    }

    /// A check under its setting: where it stands, in a line, and what can be done about it.
    struct CheckStatus: Equatable {
        let kind: CheckKind
        let state: CheckState
        let text: String
        /// Puts back what the user set up, or fixes what's broken.
        let restore: FixButton?
        /// Accepts the change as the user's choice.
        let accept: FixButton?

        /// A check that needs the user: drift, or broken.
        var isProblem: Bool { Settings.isProblem(state) }
    }

    /// An app's or site's indefinite rule in the Rules tab: what it says, where it comes from or
    /// until when it's snoozed, and what can be done about it.
    struct RuleRow: Equatable {
        let identity: RuleIdentity
        let says: OnOff
        /// "From your config", "Ad-hoc" or "Snoozed until 07:00".
        let label: String
        let snoozed: Bool
        /// None for a config rule that isn't snoozed: only the config changes it (ADR-0006).
        let action: Action?
    }

    /// An app or site that temporary rules alone keep on or off for now, and ending them.
    struct ForNowRow: Equatable {
        let identity: RuleIdentity
        /// "Night Shift off until 20:00".
        let label: String
        let action: Action
    }

    let websiteChecking: Switch
    /// "Open at login", in installs that open themselves at login (ADR-0011).
    let openAtLogin: Switch?
    /// The checks the window shows: Accessibility's while website checking needs it, opening at
    /// login's and the ad-hoc rules' only while they're problems.
    let checks: [CheckStatus]
    /// Where the config is read from, whether or not it exists.
    let configPath: String
    /// The apps' and sites' indefinite rules: the config's, then the user's. The tour's never
    /// show (ADR-0016).
    let rules: [RuleRow]
    /// The apps and sites Night Shift is on or off in for now, by temporary rules alone: a
    /// snoozed rule shows on its own row, and everywhere's on the menu's Night Shift item.
    let forNow: [ForNowRow]
    /// The version of the Nightcap that answers.
    let version: String

    /// Settings as `core` answers its queries now.
    init(_ core: Core, words: Wording) {
        let preferences = core.preferences()
        let answered = core.checks()
        let checks = answered.checks
        let rules = core.rules()
        let websites = preferences.websiteChecking
        let on = websites.state == .on
        websiteChecking = Switch(on: on, locked: websites.fromConfig, command: .websites(on ? .off : .on))
        // Only an install that opens itself at login has the check. Registered but waiting for
        // approval counts as on; its check explains the rest.
        openAtLogin =
            checks.contains(where: { Self.kind(of: $0.detail) == .openAtLogin })
            ? answered.permissions.loginItem.map { item in
                let on = item != .notRegistered
                return Switch(on: on, locked: false, command: .openAtLogin(on ? .off : .on))
            } : nil
        self.checks = checks.filter(Self.shows).map { check in
            let fixes = Self.fixes(for: check.detail, websitesFromConfig: websites.fromConfig)
            let button = { (fix: Fix?) in fix.map { FixButton(title: words.title(of: $0), fix: $0) } }
            return CheckStatus(
                kind: Self.kind(of: check.detail), state: check.state, text: words.check(check.detail),
                restore: button(fixes.restore), accept: button(fixes.accept))
        }
        configPath = core.config().path
        self.rules = Self.rows(of: rules, words: words)
        forNow = Self.forNow(rules).compactMap { place, rules in
            Self.identity(of: place).map { identity in
                ForNowRow(
                    identity: identity,
                    label: words.forNow(rules.map { Wording.Change(state: $0.says, until: Self.end(of: $0.length)) }),
                    action: Action(title: words.end, command: .reset(place)))
            }
        }
        version = core.about().version
    }

    /// The check of `kind` the window shows, if any.
    func check(_ kind: CheckKind) -> CheckStatus? {
        checks.first { $0.kind == kind }
    }

    /// Whether `tab` shows a problem, which marks it.
    func hasProblem(in tab: Tab) -> Bool {
        checks.contains { tab.checks.contains($0.kind) && $0.isProblem }
    }

    /// The Rules tab's rows for `rules`, every rule at every place: the apps' and sites'
    /// indefinite rules, the config's then the user's, each with its label and what can be done
    /// about it. Onboarding's suggestions say where their rules come from in the same words.
    static func rows(of rules: [Rule], words: Wording) -> [RuleRow] {
        rules.filter(isAppOrSiteRule).compactMap { rule in row(rule, among: rules, words) }
    }

    /// The tab with the problem the menu points at: General's first, then Rules'.
    static func tab(for checks: [Check]) -> Tab {
        let problems = checks.filter { isProblem($0.state) }.map { kind(of: $0.detail) }
        return problems.isEmpty || problems.contains(where: Tab.general.checks.contains) ? .general : .rules
    }

    // MARK: Deciding

    private static func isProblem(_ state: CheckState) -> Bool {
        state == .drifted || state == .broken
    }

    /// Whether the window shows a check: Accessibility's goes unmentioned while website checking
    /// doesn't need it, opening at login's switch says whether Nightcap opens at login, and the
    /// ad-hoc rules' file goes unmentioned while nothing's wrong with it.
    private static func shows(_ check: Check) -> Bool {
        switch check.detail {
        case .accessibility(.notNeeded): false
        case .openAtLogin, .adHocRules: isProblem(check.state)
        case .nightShift, .accessibility, .notifications, .configFile: true
        }
    }

    /// What Settings offers for what a check found: putting back what the user set up, and
    /// accepting the change as their choice.
    private static func fixes(for detail: CheckDetail, websitesFromConfig: Bool) -> (restore: Fix?, accept: Fix?) {
        switch detail {
        // Only the config can turn off what the config turned on.
        case .accessibility(.notAllowed): (.askForAccessibility, websitesFromConfig ? nil : .stopCheckingWebsites)
        case .notifications(.switchedOff): (.allowNotifications, .keepNotificationsOff)
        case .notifications(.off): (.allowNotifications, nil)
        case .nightShift(.setAside): (.openLocationServices, .keepScheduleOff)
        // Registering again doesn't help an item that's waiting for approval.
        case .openAtLogin(.awaitingApproval): (.openLoginItems, .dontOpenAtLogin)
        case .openAtLogin(.removed): (.openAtLogin, .dontOpenAtLogin)
        case .configFile(.error): (.reload, nil)
        case .nightShift(.available), .nightShift(.unavailable), .nightShift(.noSunTimes), .accessibility(.allowed),
            .accessibility(.notNeeded), .notifications(.allowed), .openAtLogin(.opens), .openAtLogin(.off),
            .configFile(.valid), .configFile(.missing), .adHocRules:
            (nil, nil)
        }
    }

    /// A rule's row: a snooze can be put back, the user's rule removed, and a config rule changed
    /// only in the config (ADR-0006). The tour's rules have none (ADR-0016).
    private static func row(_ rule: Rule, among rules: [Rule], _ words: Wording) -> RuleRow? {
        guard let identity = identity(of: rule.place) else { return nil }
        // The shortest rule at the place, where it says the other way.
        let snooze = rules.filter { $0.place == rule.place && $0.length != .indefinite }
            .min { rank(of: $0.length) < rank(of: $1.length) }
            .flatMap { $0.says != rule.says ? $0 : nil }
        let shown: (label: String, action: Action?)
        switch (snooze, rule.setBy) {
        case (_, .tour), (_, .outsideChange):
            return nil
        case (let snooze?, _):
            shown = (
                words.snoozed(snooze.says, until: end(of: snooze.length)),
                Action(title: words.putBack, command: .reset(rule.place))
            )
        case (nil, .config):
            shown = (words.fromYourConfig, nil)
        case (nil, .you):
            shown = (words.adHoc, Action(title: words.remove, command: .removeRule(identity)))
        }
        return RuleRow(
            identity: identity, says: rule.says, label: shown.label, snoozed: snooze != nil, action: shown.action)
    }

    /// The apps and sites with temporary rules and no indefinite one, each with its rules,
    /// shortest first, in the order the places got their first.
    private static func forNow(_ rules: [Rule]) -> [(Place, [Rule])] {
        var places: [(place: Place, rules: [Rule])] = []
        for rule in rules where rule.place != .everywhere && rule.length != .indefinite {
            guard !rules.contains(where: { $0.place == rule.place && isAppOrSiteRule($0) }) else { continue }
            if let index = places.firstIndex(where: { $0.place == rule.place }) {
                places[index].rules.append(rule)
            } else {
                places.append((rule.place, [rule]))
            }
        }
        return places.map { ($0.place, $0.rules.sorted { rank(of: $0.length) < rank(of: $1.length) }) }
    }

    /// Whether a rule is an app's or a site's indefinite one, as the Rules tab lists them.
    private static func isAppOrSiteRule(_ rule: Rule) -> Bool {
        rule.place != .everywhere && rule.length == .indefinite
    }

    /// The identity a rule for this place has; everywhere has none.
    private static func identity(of place: Place) -> RuleIdentity? {
        switch place {
        case .everywhere: nil
        case .app(let bundleId): .app(bundleId)
        case .domain(let domain): .domain(domain)
        }
    }

    /// When a rule of this length ends, if it has an end.
    private static func end(of length: Length) -> Date? {
        switch length {
        case .indefinite: nil
        case .untilScheduleTurns(let until): until
        case .until(let until): until
        }
    }

    /// Where a rule's length puts it among its place's, shortest first.
    private static func rank(of length: Length) -> Int {
        switch length {
        case .until: 0
        case .untilScheduleTurns: 1
        case .indefinite: 2
        }
    }

    private static func kind(of detail: CheckDetail) -> CheckKind {
        switch detail {
        case .nightShift: .nightShift
        case .accessibility: .accessibility
        case .notifications: .notifications
        case .openAtLogin: .openAtLogin
        case .configFile: .configFile
        case .adHocRules: .adHocRules
        }
    }
}
