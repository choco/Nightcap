import NightcapCore

/// Onboarding's flow, apart from its drawing: the steps this user takes, where they are among
/// them, what each step's buttons do, and what its choices send to the core, and when. The
/// strength goes as its slider moves and the schedule on Continue (ADR-0014), rules on Add,
/// opening at login on the last things' Continue (ADR-0011), and website checking only as
/// onboarding finishes (ADR-0012).
final class OnboardingFlow {
    /// What onboarding asks this user, from the core's queries as it opens: its Night Shift steps
    /// (ADR-0014), and what the config or the install already answers.
    struct Setup: Equatable {
        /// How warm, unless the config sets it or the strength can't be read.
        let askStrength: Bool
        /// When it comes on, unless the config sets it or Night Shift is unavailable.
        let askSchedule: Bool
        /// On a schedule already, so onboarding offers to skip to Nightcap's part.
        let alreadySetUp: Bool
        /// The Mac's values, which the steps start from.
        let strength: UInt8?
        let schedule: Schedule?
        /// Website checking as the config sets it, which answers its step for good.
        let websitesFromConfig: Bool?
        /// Whether Nightcap opens at login now, or nil where it never opens itself.
        let opensAtLogin: Bool?
    }

    enum Step: Equatable {
        /// Night Shift is on a schedule already: the user keeps it, or reviews it.
        case alreadySetUp
        case strength, schedule, apps, websites
        /// Notifications and opening at login.
        case lastThings
        case ready
    }

    /// The websites step asks first, then for Accessibility, then for the sites.
    enum WebsitesPage: Equatable {
        case ask, accessibility, sites
    }

    /// The steps' two groups, Night Shift's and Nightcap's. Ready sums them up, in neither.
    enum Part: CaseIterable {
        case nightShift, nightcap

        var steps: [Step] {
            switch self {
            case .nightShift: [.alreadySetUp, .strength, .schedule]
            case .nightcap: [.apps, .websites, .lastThings]
            }
        }
    }

    /// A footer's button, by what it does.
    enum FooterButton: Equatable {
        /// Moves on, sending what the step chose.
        case `continue`
        /// Moves on, leaving the step's choice unmade.
        case notNow
        case reviewSettings, skipToNightcap
        /// Adds a rule for each ticked suggestion without one, then moves on.
        case addRules(count: Int)
        /// Adds an app the user picks to the apps step; the window asks which.
        case chooseApp
        case turnOnWebsites, allowAccessibility
        /// Waits for the Accessibility grant.
        case waiting
        /// Closes the window, which finishes onboarding.
        case done

        var enabled: Bool {
            switch self {
            case .addRules(let count): count > 0
            case .waiting: false
            default: true
            }
        }
    }

    /// A step's buttons: a link beside Back, then the one to press last on the right.
    struct Footer: Equatable {
        var link: FooterButton? = nil
        var secondary: FooterButton? = nil
        let primary: FooterButton
    }

    /// A step's dot: done, current or to do, and the index a click on it goes to, for a step
    /// reached before other than the current one.
    struct Dot: Equatable {
        enum State { case current, done, todo }
        let step: Step
        let state: State
        let go: Int?
    }

    /// An app or site offered a rule, and where its rule comes from when it has one already, such
    /// as "From your config".
    struct Suggestion: Equatable {
        let identity: RuleIdentity
        let ruled: String?
    }

    /// Three rows of three fit the apps step.
    static let gridSize = 9

    let setup: Setup
    /// The bundle IDs of the suggested apps on this Mac, then of any the user chose.
    private var appIds: [String]
    private let domains: [String]
    private var ticked: Set<RuleIdentity> = []
    /// What onboarding gave a rule, whose suggestions stay as the user ticked them.
    private var given: Set<RuleIdentity> = []
    private(set) var reviewing = false
    private(set) var page = WebsitesPage.ask
    private(set) var waitingForAccessibility = false
    /// Whether the user chose website checking, which onboarding sends only as it finishes.
    private(set) var websites: Bool?
    /// The strength as last set here or read from the Mac, 0 to 100.
    private(set) var strength: UInt8
    /// The schedule the user picked, which Continue writes; it starts at the Mac's, or at sunset to
    /// sunrise where Night Shift has none.
    private(set) var schedule: Schedule
    var customHours: (from: TimeOfDay, to: TimeOfDay)
    var wantsNotifications = false
    /// Whether Nightcap is to open at login; nil where it never opens itself.
    var opensAtLogin: Bool?
    private var position = 0
    /// The furthest step reached, whose dot and those before it can be clicked.
    private var reached = 0

    /// The core's rules, as the snapshot onboarding last took has them.
    private var rules: [ListedRule]
    /// The grants, as the snapshot onboarding last took has them.
    private var permissions: Permissions
    /// Whether the config sets website checking, as the snapshot onboarding last took has it.
    private var configSetsWebsites: Bool
    private let words: Wording
    private let askForAccessibility: () -> Void
    private let choose: (Command) -> Void
    private let askForNotifications: () -> Void
    private let onFinish: (_ websites: OnOff?) -> Void

    /// `snapshot` is the core as onboarding opens, which decides what onboarding asks this user
    /// and has the rules and the grants as they stand. `apps` are the bundle IDs of the suggested
    /// apps the grid shows, and `sites` the suggested sites' domains; `words` say where a rule
    /// comes from; `choose` sends a command to the core, as the menu does; `onFinish` runs once
    /// onboarding finishes, with the website checking it sent, if any.
    init(
        _ snapshot: Snapshot,
        apps: [String],
        sites: [String],
        words: Wording,
        askForAccessibility: @escaping () -> Void,
        choose: @escaping (Command) -> Void,
        askForNotifications: @escaping () -> Void,
        onFinish: @escaping (_ websites: OnOff?) -> Void
    ) {
        setup = Setup(snapshot)
        rules = snapshot.rules()
        permissions = snapshot.checks().permissions
        configSetsWebsites = setup.websitesFromConfig != nil
        self.words = words
        self.askForAccessibility = askForAccessibility
        self.choose = choose
        self.askForNotifications = askForNotifications
        self.onFinish = onFinish
        appIds = apps
        domains = sites
        opensAtLogin = setup.opensAtLogin
        strength = setup.strength ?? 50
        let current = setup.schedule ?? .off
        if case .custom(let from, let to) = current {
            customHours = (from, to)
        } else {
            customHours = (TimeOfDay(hour: 22, minute: 0), TimeOfDay(hour: 7, minute: 0))
        }
        schedule = current == .off ? .sunsetToSunrise : current
        // The config answers website checking for good; it can still need Accessibility.
        if setup.websitesFromConfig == true {
            websites = true
            page = accessibilityGranted ? .sites : .accessibility
        }
    }

    /// Takes the rules and the grants as `snapshot` has them, the one each redraw after an
    /// outcome hands on. What onboarding asks stays as it opened. The Accessibility page gives
    /// way to the sites as the grant arrives while it doesn't show; while it shows, it shows the
    /// grant first (`grantArrived`).
    func refresh(_ snapshot: Snapshot) {
        rules = snapshot.rules()
        permissions = snapshot.checks().permissions
        configSetsWebsites = snapshot.config().websites != nil
        if step != .websites, page == .accessibility, accessibilityGranted {
            waitingForAccessibility = false
            page = .sites
        }
    }

    var accessibilityGranted: Bool { permissions.accessibility }
    /// Whether notifications are allowed, denied or not asked for yet.
    var notifications: NotificationPermission { permissions.notifications }

    // MARK: Where onboarding is

    /// The steps this user takes, in order: Night Shift's unless the config sets it, then
    /// Nightcap's.
    var steps: [Step] {
        var steps: [Step] = []
        if setup.alreadySetUp && !reviewing {
            steps.append(.alreadySetUp)
        } else {
            if setup.askStrength { steps.append(.strength) }
            if setup.askSchedule { steps.append(.schedule) }
        }
        steps.append(.apps)
        if setup.websitesFromConfig != false { steps.append(.websites) }
        if notifications != .granted || opensAtLogin != nil { steps.append(.lastThings) }
        steps.append(.ready)
        return steps
    }

    /// Where onboarding is among `steps`, which can shrink under it: allowing notifications drops
    /// the last things where there's no login item to offer, and onboarding moves on to Ready.
    var index: Int { min(position, steps.count - 1) }

    var step: Step { steps[index] }

    /// The step's group and its place in it, from 0, among how many it has; nil on Ready.
    var placeInPart: (part: Part, place: Int, count: Int)? {
        let step = self.step
        guard let part = Part.allCases.first(where: { $0.steps.contains(step) }) else { return nil }
        let group = steps.filter(part.steps.contains)
        return (part, group.firstIndex(of: step)!, group.count)
    }

    /// A dot per step in two groups, Night Shift's and Nightcap's, leaving out a group this user
    /// skips.
    var dots: [[Dot]] {
        let steps = self.steps
        let index = self.index
        return Part.allCases.map { part in
            steps.enumerated().filter { part.steps.contains($0.element) }.map { place, step in
                Dot(
                    step: step,
                    state: place == index ? .current : place < index ? .done : .todo,
                    go: place != index && place <= reached ? place : nil)
            }
        }.filter { !$0.isEmpty }
    }

    func go(to index: Int) {
        position = index
        reached = max(reached, index)
    }

    private func advance() {
        go(to: index + 1)
    }

    /// Whether Back goes anywhere: to an earlier page of the websites step, or the step before.
    var canGoBack: Bool {
        backToAsk || reviewing || index > 0
    }

    private var backToAsk: Bool {
        step == .websites && page != .ask && setup.websitesFromConfig == nil
    }

    func goBack() {
        if backToAsk {
            page = .ask
        } else if index == 0, reviewing {
            reviewing = false
            go(to: 0)
        } else if index > 0 {
            go(to: index - 1)
        }
    }

    // MARK: What the buttons do

    var footer: Footer {
        switch step {
        case .alreadySetUp:
            Footer(secondary: .reviewSettings, primary: .skipToNightcap)
        case .strength, .schedule, .lastThings:
            Footer(primary: .continue)
        case .apps:
            Footer(
                link: apps.isEmpty || apps.count >= Self.gridSize ? nil : .chooseApp, secondary: .notNow,
                primary: .addRules(count: adding.count))
        case .websites:
            switch page {
            case .ask: Footer(secondary: .notNow, primary: .turnOnWebsites)
            case .accessibility where waitingForAccessibility || accessibilityGranted: Footer(primary: .waiting)
            case .accessibility: Footer(secondary: .notNow, primary: .allowAccessibility)
            case .sites: Footer(secondary: .notNow, primary: .addRules(count: adding.count))
            }
        case .ready:
            Footer(primary: .done)
        }
    }

    /// Carries out a footer's button, apart from those the window carries out: choosing an app
    /// and closing.
    func perform(_ button: FooterButton) {
        switch button {
        case .continue:
            switch step {
            case .schedule where schedule != setup.schedule:
                choose(.setSchedule(schedule))
            case .lastThings:
                if let opensAtLogin { choose(.openAtLogin(opensAtLogin ? .on : .off)) }
                if wantsNotifications, notifications == .notRequested { askForNotifications() }
            default:
                break
            }
            advance()
        case .notNow:
            if step == .websites, page == .ask { websites = false }
            advance()
        case .reviewSettings:
            reviewing = true
            go(to: 0)
        case .skipToNightcap:
            go(to: steps.firstIndex(where: Part.nightcap.steps.contains)!)
        case .addRules:
            for suggestion in adding {
                choose(.addRule(identity: suggestion.identity, says: .off))
                given.insert(suggestion.identity)
            }
            advance()
        case .turnOnWebsites:
            websites = true
            page = accessibilityGranted ? .sites : .accessibility
        case .allowAccessibility:
            waitingForAccessibility = true
            askForAccessibility()
        case .chooseApp, .waiting, .done:
            break
        }
    }

    /// Onboarding ends, by Done or the close button, with what was chosen so far. Website checking
    /// goes as a command, on only where the user turned it on, wherever onboarding asks it: where
    /// the config set it neither as onboarding opened nor in the last snapshot, so the core has no
    /// reason to refuse it (ADR-0012).
    func finish() {
        let asked = setup.websitesFromConfig == nil && !configSetsWebsites
        let sent: OnOff? = asked ? (websites == true ? .on : .off) : nil
        if let sent { choose(.websites(sent)) }
        onFinish(sent)
    }

    // MARK: The steps' choices

    /// The suggested apps on this Mac, then any the user chose. One with a rule, short of a rule
    /// given here, comes ticked and can't be changed, as the last snapshot has the rules.
    var apps: [Suggestion] {
        let ruled = self.ruled
        return appIds.map { Suggestion(identity: .app($0), ruled: ruled[.app($0)]) }
    }

    /// The suggested sites, the ones with a rule likewise.
    var sites: [Suggestion] {
        let ruled = self.ruled
        return domains.map { Suggestion(identity: .domain($0), ruled: ruled[.domain($0)]) }
    }

    /// Where each app's or site's rule comes from, as the Rules tab says it, short of those given
    /// here.
    private var ruled: [RuleIdentity: String] {
        Dictionary(
            Settings.rows(of: rules, words: words).filter { !given.contains($0.identity) }.map {
                ($0.identity, $0.label)
            }, uniquingKeysWith: { first, _ in first })
    }

    /// The slider's strength: a preview while it's dragged, saved otherwise, as the menu's does.
    func moveStrength(to value: UInt8, previewing: Bool) {
        strength = value
        choose(previewing ? .previewStrength(value) : .strength(value))
    }

    func pick(_ schedule: Schedule) {
        self.schedule = schedule
    }

    func isTicked(_ suggestion: Suggestion) -> Bool {
        suggestion.ruled != nil || ticked.contains(suggestion.identity)
    }

    func toggle(_ suggestion: Suggestion) {
        if ticked.remove(suggestion.identity) == nil { ticked.insert(suggestion.identity) }
    }

    /// An app the user chose, added to the grid ticked. One with a rule already shows where its rule
    /// comes from and can't be changed, as a suggestion with one does.
    func add(app bundleId: String) {
        if !appIds.contains(bundleId) { appIds.append(bundleId) }
        ticked.insert(.app(bundleId))
    }

    /// The ticked suggestions without a rule on the step's grid.
    private var adding: [Suggestion] {
        let grid =
            switch step {
            case .apps: apps
            case .websites: sites
            default: [Suggestion]()
            }
        return grid.filter { $0.ruled == nil && ticked.contains($0.identity) }
    }

    /// The suggested apps given a rule here, as the last snapshot has them.
    var addedApps: [Suggestion] { added(apps) }
    var addedSites: [Suggestion] { added(sites) }

    private func added(_ suggestions: [Suggestion]) -> [Suggestion] {
        let ruled = Set(Settings.rows(of: rules, words: words).map(\.identity))
        return suggestions.filter { given.contains($0.identity) && ruled.contains($0.identity) }
    }

    // MARK: The Accessibility grant

    /// The grant arrived while the Accessibility page shows it's asked for; true if the page is to
    /// show it.
    func grantArrived() -> Bool {
        guard step == .websites, page == .accessibility, accessibilityGranted else { return false }
        waitingForAccessibility = false
        return true
    }

    /// The Accessibility page gives way to the sites, while it still shows; true if it did.
    func moveOnToSites() -> Bool {
        guard step == .websites, page == .accessibility else { return false }
        page = .sites
        return true
    }

    #if DEBUG
    /// Jumps to `step` for a snapshot; false if this user doesn't take it.
    func show(_ step: Step, page: WebsitesPage, reviewing: Bool) -> Bool {
        self.reviewing = reviewing
        guard let index = steps.firstIndex(of: step) else { return false }
        position = index
        reached = index
        self.page = page
        return true
    }
    #endif
}

extension OnboardingFlow.Setup {
    /// What onboarding asks, as `snapshot` answers its queries: the strength unless the config sets
    /// it or it can't be read, and the schedule unless the config sets it or Night Shift is
    /// unavailable (ADR-0014).
    init(_ snapshot: Snapshot) {
        let nightShift = snapshot.nightShift()
        let config = snapshot.config()
        let checks = snapshot.checks()
        let askStrength = nightShift.strength != nil && config.strength == nil
        let askSchedule = nightShift.state != .unavailable && config.schedule == nil
        let opensItself = snapshot.about().managesItself
        self.init(
            askStrength: askStrength,
            askSchedule: askSchedule,
            alreadySetUp: (askStrength || askSchedule) && nightShift.schedule.map { $0 != .off } == true,
            strength: nightShift.strength,
            schedule: nightShift.schedule,
            websitesFromConfig: config.websites.map { $0 == .on },
            opensAtLogin: opensItself ? checks.permissions.loginItem.map { $0 != .notRegistered } : nil)
    }
}
