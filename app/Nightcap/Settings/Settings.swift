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
    struct CommandButton: Equatable {
        let title: String
        let command: Command
    }

    /// What a fix's button does: sends the command that restores, accepts or reloads, or carries
    /// out one of the app's own fixes.
    enum Fix: Equatable {
        case command(Command)
        case app(AppFix)
    }

    /// A button that carries out `fix`.
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
        var isProblem: Bool { state.isProblem() }
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
        let button: CommandButton?
    }

    /// An app or site that a temporary rule alone keeps on or off for now, and ending it.
    struct ForNowRow: Equatable {
        let identity: RuleIdentity
        /// "Night Shift off until 20:00".
        let label: String
        let button: CommandButton
    }

    let websiteChecking: Switch
    /// "Open at login", in installs that open themselves at login (ADR-0011).
    let openAtLogin: Switch?
    /// The checks the window shows: Accessibility's while website checking needs it, opening at
    /// login's and the ad-hoc rules' only while they're problems.
    let checks: [CheckStatus]
    /// Where the config is read from, whether or not it exists.
    let configPath: String
    /// The apps' and sites' indefinite rules: the config's, then the user's. A trial's never
    /// show (ADR-0016).
    let rules: [RuleRow]
    /// The apps and sites Night Shift is on or off in for now, by a temporary rule alone: a
    /// snoozed rule shows on its own row, and everywhere's on the menu's Night Shift item.
    let forNow: [ForNowRow]
    /// The version of the Nightcap that answers.
    let version: String

    /// Settings as `snapshot` answers its queries.
    init(_ snapshot: Snapshot, words: Wording) {
        let preferences = snapshot.preferences()
        let answered = snapshot.checks()
        let checks = answered.checks
        let rules = snapshot.rules()
        let websites = preferences.websiteChecking
        let on = websites.state == .on
        websiteChecking = Switch(on: on, locked: websites.fromConfig, command: .websites(on ? .off : .on))
        // Registered but waiting for approval counts as on; its check explains the rest.
        openAtLogin =
            snapshot.about().install == .selfManaged
            ? answered.permissions.loginItem.map { item in
                let on = item != .notRegistered
                return Switch(on: on, locked: false, command: .openAtLogin(on ? .off : .on))
            } : nil
        self.checks = checks.filter(Self.shows).map { check in
            let fixes = Self.fixes(for: check.detail, websitesFromConfig: websites.fromConfig, words: words)
            return CheckStatus(
                kind: check.detail.kind(), state: check.state, text: words.check(check.detail),
                restore: fixes.restore, accept: fixes.accept)
        }
        configPath = snapshot.config().path
        self.rules = Self.rows(of: rules, words: words)
        forNow = Self.forNow(rules.map(\.rule)).compactMap { rule in
            rule.place.identity().map { identity in
                ForNowRow(
                    identity: identity,
                    label: words.forNow(Wording.TemporaryRule(state: rule.says, until: rule.length.end())),
                    button: CommandButton(title: words.end, command: .reset(rule.place)))
            }
        }
        version = snapshot.about().version
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
    static func rows(of rules: [ListedRule], words: Wording) -> [RuleRow] {
        rules.filter { isAppOrSiteRule($0.rule) }.compactMap { listed in row(listed, words) }
    }

    /// The tab with the problem the menu points at: General's first, then Rules'.
    static func tab(for checks: [Check]) -> Tab {
        let problems = checks.filter { $0.state.isProblem() }.map { $0.detail.kind() }
        return problems.isEmpty || problems.contains(where: Tab.general.checks.contains) ? .general : .rules
    }

    // MARK: Deciding

    /// Whether the window shows a check: Accessibility's goes unmentioned while website checking
    /// doesn't need it, opening at login's switch says whether Nightcap opens at login, and the
    /// ad-hoc rules' file goes unmentioned while nothing's wrong with it.
    private static func shows(_ check: Check) -> Bool {
        switch check.detail {
        case .accessibility(.notNeeded): false
        case .openAtLogin, .adHocRules: check.state.isProblem()
        case .nightShift, .accessibility, .notifications, .configFile: true
        }
    }

    /// What Settings offers for what a check found: putting back what the user set up, and
    /// accepting the change as their choice, each the app's own fix or a command.
    private static func fixes(for detail: CheckDetail, websitesFromConfig: Bool, words: Wording) -> (
        restore: FixButton?, accept: FixButton?
    ) {
        let app = { (fix: AppFix) in FixButton(title: words.title(of: fix), fix: .app(fix)) }
        let send = { (title: String, command: Command) in FixButton(title: title, fix: .command(command)) }
        return switch detail {
        // Only the config can turn off what the config turned on.
        case .accessibility(.notAllowed):
            (app(.askForAccessibility), websitesFromConfig ? nil : send(words.stopCheckingWebsites, .websites(.off)))
        case .notifications(.switchedOff):
            (app(.allowNotifications), send(words.keepNotificationsOff, .keepNotificationsOff))
        case .notifications(.off): (app(.allowNotifications), nil)
        case .nightShift(.setAside): (app(.openLocationServices), send(words.keepScheduleOff, .keepScheduleOff))
        // Registering again doesn't help an item that's waiting for approval.
        case .openAtLogin(.awaitingApproval):
            (app(.openLoginItems), send(words.dontOpenAtLogin, .openAtLogin(.off)))
        case .openAtLogin(.removed):
            (send(words.openAtLoginAgain, .openAtLogin(.on)), send(words.dontOpenAtLogin, .openAtLogin(.off)))
        case .configFile(.error): (send(words.reload, .reload), nil)
        case .nightShift(.available), .nightShift(.unavailable), .nightShift(.noSunTimes), .accessibility(.allowed),
            .accessibility(.notNeeded), .notifications(.allowed), .openAtLogin(.opens), .openAtLogin(.off),
            .configFile(.valid), .configFile(.missing), .adHocRules:
            (nil, nil)
        }
    }

    /// A rule's row: a snooze can be put back, the user's rule removed, and a config rule changed
    /// only in the config (ADR-0006). A trial's rules have none (ADR-0016).
    private static func row(_ listed: ListedRule, _ words: Wording) -> RuleRow? {
        let rule = listed.rule
        guard let identity = rule.place.identity() else { return nil }
        let shown: (label: String, button: CommandButton?)
        switch (listed.snooze, rule.setBy) {
        case (_, .trial), (_, .outsideChange):
            return nil
        case (let snooze?, _):
            shown = (
                words.snoozed(snooze.says, until: snooze.length.end()),
                CommandButton(title: words.putBack, command: .reset(rule.place))
            )
        case (nil, .config):
            shown = (words.fromYourConfig, nil)
        case (nil, .you):
            shown = (words.adHoc, CommandButton(title: words.remove, command: .removeRule(identity)))
        }
        return RuleRow(
            identity: identity, says: rule.says, label: shown.label, snoozed: listed.snooze != nil,
            button: shown.button)
    }

    /// The temporary rules at apps and sites with no indefinite one, as the rules query lists
    /// them.
    private static func forNow(_ rules: [Rule]) -> [Rule] {
        rules.filter { rule in
            rule.place != .everywhere && rule.length != .indefinite
                && !rules.contains { $0.place == rule.place && isAppOrSiteRule($0) }
        }
    }

    /// Whether a rule is an app's or a site's indefinite one, as the Rules tab lists them.
    private static func isAppOrSiteRule(_ rule: Rule) -> Bool {
        rule.place != .everywhere && rule.length == .indefinite
    }
}

/// A fix the app carries out itself, never the core, as it happens in macOS: a prompt for a
/// grant, or the pane of System Settings where the user puts back what they set up. Every other
/// fix is a command.
enum AppFix: Equatable {
    /// The macOS prompt while Nightcap has no entry in the list, else the pane.
    case askForAccessibility
    /// The macOS prompt while notifications were never asked for, else Nightcap's page in
    /// Notification settings.
    case allowNotifications
    /// Login Items in System Settings, where macOS asks for its approval.
    case openLoginItems
    /// Location Services in System Settings.
    case openLocationServices
}
