import Foundation
import NightcapCore
import Testing

private let imovie = "com.apple.iMovieApp"
private let photos = "com.apple.Photos"
private let tv = "com.apple.TV"
private let books = "com.apple.iBooksX"

/// What onboarding sent the core, in order.
private final class Sent {
    var commands: [Command] = []
}

/// A self-managed release, or a managed install, or the dev build as `bundleId`, that has read the
/// Mac's Night Shift at a strength of 60, found Nightcap not opening at login, and heard of
/// Accessibility as `accessibility` and of notifications as `notifications`, with `config` as its
/// config and `rules` as its ad-hoc rules file, then `after`.
private func core(
    schedule: Schedule = .off, bundleId: String = "io.github.choco.nightcap", bundleWritable: Bool = true,
    config: String? = nil, rules: String? = nil, accessibility: Bool = true,
    notifications: NotificationPermission = .notRequested, after: [Step] = []
) throws -> Core {
    try Core.staged(
        scenario: Scenario(
            now: Date(), bundleId: bundleId, bundleWritable: bundleWritable, config: config, rules: rules,
            steps: [
                .fact(.statusChanged(enabled: false, schedule: schedule, sunScheduleSetAside: false, writtenAt: nil)),
                .fact(.strengthChanged(value: 60)),
                .fact(.loginItemChanged(item: .notRegistered)),
                permissions(accessibility, notifications),
            ] + after))
}

/// What onboarding asks on `core`.
private func setup(_ core: Core) -> OnboardingFlow.Setup {
    OnboardingFlow.Setup(core.snapshot())
}

/// The flow over `core`, with iMovie and Photos to suggest and figma.com among the sites. What it
/// chooses reaches the core, as the menu's commands do, and `sent` too; the core's snapshot after
/// each reaches the flow, as the app's redraw hands it on.
private func onboarding(
    _ core: Core, sent: Sent = Sent(), onFinish: @escaping (OnOff?) -> Void = { _ in }
) -> OnboardingFlow {
    weak var made: OnboardingFlow?
    let flow = OnboardingFlow(
        core.snapshot(), apps: [imovie, photos], sites: ["figma.com"], words: .staged, askForAccessibility: {},
        choose: { command in
            sent.commands.append(command)
            core.send(command: command)
            made?.refresh(core.snapshot())
        },
        askForNotifications: {}, onFinish: onFinish)
    made = flow
    return flow
}

/// Onboarding's flow on a staged core: what each user is asked, and what reaches the core when.
struct OnboardingFlowTests {
    @Test func onboardingAsksAboutNightShiftStartingFromTheMacsValues() throws {
        let core = try Core.staged(
            scenario: Scenario(
                now: at("2026-10-16T12:00:00+02:00"),
                steps: [
                    .fact(.statusChanged(enabled: false, schedule: .off, sunScheduleSetAside: false, writtenAt: nil)),
                    .fact(.strengthChanged(value: 60)),
                ]))

        #expect(
            setup(core)
                == OnboardingFlow.Setup(
                    askStrength: true, askSchedule: true, alreadySetUp: false, strength: 60, schedule: .off,
                    websitesFromConfig: nil, opensAtLogin: nil))
        let flow = onboarding(core)
        #expect(flow.strength == 60)
        #expect(flow.schedule == .sunsetToSunrise)
    }

    @Test func onboardingAsksAboutNightShiftWhatTheConfigLeavesAndTheMacCanDo() throws {
        let asks = { (config: String?, reported: [Step]) in
            let setup = OnboardingFlow.Setup(
                try Core.staged(
                    scenario: Scenario(now: at("2026-10-16T12:00:00+02:00"), config: config, steps: reported)
                )
                .snapshot())
            return [setup.askStrength, setup.askSchedule]
        }
        let status = Step.fact(
            .statusChanged(enabled: false, schedule: .off, sunScheduleSetAside: false, writtenAt: nil))
        let reported = [status, .fact(.strengthChanged(value: 60))]

        #expect(try asks(nil, []) == [false, false])
        #expect(try asks(nil, reported) == [true, true])
        #expect(try asks("[night_shift]\nstrength = 40\n", reported) == [false, true])
        #expect(try asks("[night_shift]\nschedule = \"off\"\n", reported) == [true, false])
        #expect(try asks(nil, [status]) == [false, true])
    }

    @Test func onboardingOffersToSkipNightShiftOnceItsOnASchedule() throws {
        #expect(setup(try core(schedule: .sunsetToSunrise)).alreadySetUp)
        #expect(!setup(try core(schedule: .off)).alreadySetUp)
    }

    @Test func onboardingLeavesOutWhatTheConfigSets() throws {
        let staged = try core(config: "websites = true\n[night_shift]\nstrength = 40\n")

        let asked = setup(staged)
        #expect(!asked.askStrength)
        #expect(asked.askSchedule)
        #expect(asked.websitesFromConfig == true)
        #expect(onboarding(staged).steps == [.schedule, .apps, .websites, .lastThings, .ready])
    }

    @Test func onlyAnInstallThatOpensItselfAtLoginAsksToOpenAtLogin() throws {
        #expect(setup(try core()).opensAtLogin == false)
        #expect(setup(try core(bundleWritable: false)).opensAtLogin == nil)
        #expect(setup(try core(bundleId: "io.github.choco.nightcap.dev")).opensAtLogin == nil)
    }

    @Test func aLoginItemRegisteredButWaitingForApprovalStartsOpeningAtLoginTicked() throws {
        let flow = onboarding(try selfManaged(.requiresApproval))

        #expect(flow.setup.opensAtLogin == true)
        #expect(flow.opensAtLogin == true)
    }

    @Test func aNewUserIsAskedAboutNightShiftThenNightcap() throws {
        let flow = onboarding(try core())

        #expect(flow.steps == [.strength, .schedule, .apps, .websites, .lastThings, .ready])
    }

    @Test func aUserOnAScheduleKeepsItOrReviewsIt() throws {
        let flow = onboarding(try core(schedule: .sunsetToSunrise))
        #expect(flow.steps == [.alreadySetUp, .apps, .websites, .lastThings, .ready])

        flow.perform(.reviewSettings)

        #expect(flow.steps == [.strength, .schedule, .apps, .websites, .lastThings, .ready])
        #expect(flow.step == .strength)
    }

    @Test func whatTheConfigSetsIsNeverAsked() throws {
        let config = """
            websites = false
            [night_shift]
            strength = 40
            schedule = "sunset-to-sunrise"
            """
        let flow = onboarding(try core(config: config))

        #expect(flow.steps == [.apps, .lastThings, .ready])
    }

    @Test func aManagedInstallWithNotificationsAllowedHasNoLastThings() throws {
        let flow = onboarding(try core(bundleWritable: false, notifications: .granted))

        #expect(flow.steps == [.strength, .schedule, .apps, .websites, .ready])
    }

    @Test func configRulesArriveTicked() throws {
        let flow = onboarding(try core(config: "[rules.off]\napp = [\"\(photos)\"]"))

        #expect(
            flow.apps == [
                OnboardingFlow.Suggestion(identity: .app(imovie), ruled: nil),
                OnboardingFlow.Suggestion(identity: .app(photos), ruled: "From your config"),
            ])
        #expect(!flow.isTicked(flow.apps[0]))
        #expect(flow.isTicked(flow.apps[1]))
    }

    @Test func addSendsARuleForEachNewlyTickedAppOnly() throws {
        let sent = Sent()
        let flow = onboarding(try core(config: "[rules.off]\napp = [\"\(photos)\"]"), sent: sent)
        flow.go(to: try #require(flow.steps.firstIndex(of: .apps)))
        flow.toggle(flow.apps[0])
        #expect(flow.footer.primary == .addRules(count: 1))

        flow.perform(.addRules(count: 1))

        #expect(sent.commands == [.addRule(identity: .app(imovie), says: .off)])
        #expect(flow.addedApps == [OnboardingFlow.Suggestion(identity: .app(imovie), ruled: nil)])
        #expect(flow.step == .websites)
    }

    @Test func aChosenAppArrivesTickedAndAddSendsItsRule() throws {
        let sent = Sent()
        let flow = onboarding(try core(), sent: sent)
        flow.go(to: try #require(flow.steps.firstIndex(of: .apps)))

        flow.add(app: tv)

        #expect(flow.apps.last == OnboardingFlow.Suggestion(identity: .app(tv), ruled: nil))
        #expect(flow.isTicked(flow.apps[2]))
        flow.perform(.addRules(count: 1))
        #expect(sent.commands == [.addRule(identity: .app(tv), says: .off)])
    }

    @Test func aChosenAppWithARuleShowsItAndAddLeavesItBe() throws {
        let sent = Sent()
        let staged = try core(
            config: "[rules.on]\napp = [\"\(tv)\"]", rules: "version = 2\n[rules.on]\napp = [\"\(books)\"]")
        let flow = onboarding(staged, sent: sent)
        flow.go(to: try #require(flow.steps.firstIndex(of: .apps)))

        flow.add(app: books)
        flow.add(app: tv)

        #expect(
            flow.apps.suffix(2) == [
                OnboardingFlow.Suggestion(identity: .app(books), ruled: "Ad-hoc"),
                OnboardingFlow.Suggestion(identity: .app(tv), ruled: "From your config"),
            ])
        #expect(flow.isTicked(flow.apps[2]))
        #expect(flow.isTicked(flow.apps[3]))
        #expect(flow.footer.primary == .addRules(count: 0))
        flow.perform(.addRules(count: 0))
        #expect(sent.commands == [])
        let rules = staged.snapshot().rules().rules.map(\.rule)
        #expect(rules.first { $0.place == .app(books) }?.says == .on)
        #expect(rules.first { $0.place == .app(tv) }?.says == .on)
    }

    @Test func aSuggestionGivenARuleElsewhereMeanwhileShowsItAndAddLeavesItBe() throws {
        let sent = Sent()
        let staged = try core()
        let flow = onboarding(staged, sent: sent)
        flow.go(to: try #require(flow.steps.firstIndex(of: .apps)))
        flow.toggle(flow.apps[0])

        staged.send(command: .addRule(identity: .app(imovie), says: .on))
        // The flow reads the snapshot it was handed until the redraw hands on the next.
        #expect(flow.apps[0] == OnboardingFlow.Suggestion(identity: .app(imovie), ruled: nil))
        flow.refresh(staged.snapshot())

        #expect(flow.apps[0] == OnboardingFlow.Suggestion(identity: .app(imovie), ruled: "Ad-hoc"))
        #expect(flow.isTicked(flow.apps[0]))
        #expect(flow.footer.primary == .addRules(count: 0))
        flow.perform(.addRules(count: 0))
        #expect(sent.commands == [])
        #expect(flow.addedApps == [])
        #expect(staged.snapshot().rules().rules.contains { $0.rule.place == .app(imovie) && $0.rule.says == .on })
    }

    @Test func theScheduleIsSentOnlyWhenItChanges() throws {
        let sent = Sent()
        let flow = onboarding(try core(schedule: .sunsetToSunrise), sent: sent)
        flow.perform(.reviewSettings)
        flow.perform(.continue)
        #expect(flow.step == .schedule)

        flow.perform(.continue)
        #expect(sent.commands == [])

        flow.goBack()
        let hours = Schedule.custom(from: TimeOfDay(hour: 21, minute: 0), to: TimeOfDay(hour: 6, minute: 30))
        flow.pick(hours)
        flow.perform(.continue)
        #expect(sent.commands == [.setSchedule(hours)])
    }

    @Test func aNewUsersScheduleStartsAtSunsetToSunriseAndIsSent() throws {
        let sent = Sent()
        let flow = onboarding(try core(), sent: sent)
        flow.go(to: try #require(flow.steps.firstIndex(of: .schedule)))

        flow.perform(.continue)

        #expect(sent.commands == [.setSchedule(.sunsetToSunrise)])
    }

    @Test func websiteCheckingIsSentOnlyAtTheFinish() throws {
        let sent = Sent()
        var finished: [OnOff?] = []
        let staged = try core()
        let flow = onboarding(staged, sent: sent, onFinish: { finished.append($0) })
        flow.go(to: try #require(flow.steps.firstIndex(of: .websites)))

        flow.perform(.turnOnWebsites)
        #expect(flow.page == .sites)
        #expect(sent.commands == [])
        #expect(finished == [])

        flow.finish()
        #expect(sent.commands == [.websites(.on)])
        #expect(finished == [.on])
        #expect(staged.snapshot().preferences().websiteChecking.state == .on)
    }

    /// Onboarding never asks it there, and the core would refuse it.
    @Test func websiteCheckingTheConfigSetsIsntSentAtTheFinish() throws {
        for config in ["websites = true\n", "websites = false\n"] {
            let sent = Sent()
            var finished: [OnOff?] = []
            let staged = try core(config: config)
            let flow = onboarding(staged, sent: sent, onFinish: { finished.append($0) })

            flow.finish()

            #expect(sent.commands == [], "\(config)")
            #expect(finished == [nil], "\(config)")
            #expect(staged.snapshot().preferences().websiteChecking.fromConfig, "\(config)")
        }
    }

    /// The same core before a reload and after it, as the steps that lead to each stage it. The
    /// core would refuse the user's choice, which the config overrules.
    @Test func websiteCheckingAConfigComesToSetWhileOnboardingIsOpenIsntSentAtTheFinish() throws {
        let sent = Sent()
        var finished: [OnOff?] = []
        let flow = onboarding(try core(config: ""), sent: sent, onFinish: { finished.append($0) })
        flow.go(to: try #require(flow.steps.firstIndex(of: .websites)))
        flow.perform(.turnOnWebsites)
        let reloaded = try core(config: "", after: [.config("websites = false\n"), .command(.reload)])

        flow.refresh(reloaded.snapshot())
        flow.finish()

        #expect(sent.commands == [])
        #expect(finished == [nil])
    }

    /// The user turns website checking on only on its step, so leaving onboarding before it leaves
    /// it off too.
    @Test func notNowLeavesWebsiteCheckingOff() throws {
        let sent = Sent()
        var finished: [OnOff?] = []
        let flow = onboarding(try core(), sent: sent, onFinish: { finished.append($0) })
        flow.go(to: try #require(flow.steps.firstIndex(of: .websites)))
        let leftEarly = Sent()
        let early = onboarding(try core(), sent: leftEarly)

        flow.perform(.notNow)
        flow.finish()
        early.finish()

        #expect(sent.commands == [.websites(.off)])
        #expect(finished == [.off])
        #expect(leftEarly.commands == [.websites(.off)])
    }

    /// The flow reads notifications from the snapshot the redraw hands it.
    @Test func allowingNotificationsWhereThereIsNoLoginItemMovesOnToReady() throws {
        let staged = try core(bundleWritable: false)
        let flow = onboarding(staged)
        flow.go(to: try #require(flow.steps.firstIndex(of: .lastThings)))
        #expect(flow.step == .lastThings)

        staged.report(fact: .permissionsChanged(accessibility: true, notifications: .granted))
        #expect(flow.step == .lastThings)
        flow.refresh(staged.snapshot())

        #expect(flow.notifications == .granted)
        #expect(!flow.steps.contains(.lastThings))
        #expect(flow.index == flow.steps.count - 1)
        #expect(flow.step == .ready)
    }

    @Test func theAccessibilityPageShowsTheGrantOnceTheSnapshotHasIt() throws {
        let staged = try core(accessibility: false)
        let flow = onboarding(staged)
        flow.go(to: try #require(flow.steps.firstIndex(of: .websites)))
        flow.perform(.turnOnWebsites)
        #expect(flow.page == .accessibility)
        #expect(flow.footer == OnboardingFlow.Footer(secondary: .notNow, primary: .allowAccessibility))
        flow.perform(.allowAccessibility)
        #expect(!flow.grantArrived())

        staged.report(fact: .permissionsChanged(accessibility: true, notifications: .notRequested))
        flow.refresh(staged.snapshot())

        #expect(flow.accessibilityGranted)
        #expect(flow.grantArrived())
        #expect(flow.page == .accessibility)
        #expect(flow.moveOnToSites())
        #expect(flow.page == .sites)
    }

    /// The config's website checking opens the step on Accessibility while the grant is missing,
    /// as the snapshot onboarding opens on has it before macOS reports the grant.
    @Test func aGrantArrivingBeforeTheWebsitesStepOpensItOnTheSites() throws {
        let staged = try core(config: "websites = true\n", accessibility: false)
        let flow = onboarding(staged)
        #expect(flow.page == .accessibility)

        staged.report(fact: .permissionsChanged(accessibility: true, notifications: .notRequested))
        flow.refresh(staged.snapshot())
        flow.go(to: try #require(flow.steps.firstIndex(of: .websites)))

        #expect(flow.page == .sites)
        #expect(flow.footer == OnboardingFlow.Footer(secondary: .notNow, primary: .addRules(count: 0)))
    }

    @Test func theDotsFollowTheStepsInTheirTwoGroups() throws {
        let flow = onboarding(try core())
        flow.perform(.continue)

        #expect(
            flow.dots == [
                [
                    OnboardingFlow.Dot(step: .strength, state: .done, go: 0),
                    OnboardingFlow.Dot(step: .schedule, state: .current, go: nil),
                ],
                [
                    OnboardingFlow.Dot(step: .apps, state: .todo, go: nil),
                    OnboardingFlow.Dot(step: .websites, state: .todo, go: nil),
                    OnboardingFlow.Dot(step: .lastThings, state: .todo, go: nil),
                ],
            ])
        let place = try #require(flow.placeInPart)
        #expect(place.part == .nightShift)
        #expect(place.place == 1)
        #expect(place.count == 2)
    }

    @Test func eachStepHasItsFooter() throws {
        let setUp = onboarding(try core(schedule: .sunsetToSunrise))
        #expect(setUp.footer == OnboardingFlow.Footer(secondary: .reviewSettings, primary: .skipToNightcap))
        setUp.perform(.skipToNightcap)
        #expect(setUp.step == .apps)

        let flow = onboarding(try core())
        #expect(flow.footer == OnboardingFlow.Footer(primary: .continue))
        flow.go(to: try #require(flow.steps.firstIndex(of: .apps)))
        #expect(
            flow.footer == OnboardingFlow.Footer(link: .chooseApp, secondary: .notNow, primary: .addRules(count: 0)))
        #expect(!flow.footer.primary.enabled)
        flow.perform(.notNow)
        #expect(flow.footer == OnboardingFlow.Footer(secondary: .notNow, primary: .turnOnWebsites))
        flow.go(to: try #require(flow.steps.firstIndex(of: .ready)))
        #expect(flow.footer == OnboardingFlow.Footer(primary: .done))
    }
}
