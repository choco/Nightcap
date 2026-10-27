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

/// A self-managed release, or a managed install, that has read the Mac's Night Shift at a strength
/// of 60 and found Nightcap not opening at login, with `config` as its config and `rules` as its
/// ad-hoc rules file.
private func core(
    schedule: Schedule = .off, bundleWritable: Bool = true, config: String? = nil, rules: String? = nil
) throws -> Core {
    try Core.staged(
        scenario: Scenario(
            now: Date(), bundleWritable: bundleWritable, config: config, rules: rules,
            steps: [
                .event(.statusChanged(enabled: false, schedule: schedule, sunScheduleSetAside: false)),
                .event(.strengthChanged(value: 60)),
                .event(.loginItemChanged(item: .notRegistered)),
            ]))
}

/// The flow over `core`, with iMovie and Photos to suggest and figma.com among the sites. What it
/// chooses reaches the core, as the menu's commands do, and `sent` too.
private func onboarding(
    _ core: Core, sent: Sent = Sent(),
    notifications: @escaping () -> NotificationPermission = { .notRequested },
    onFinish: @escaping (Bool) -> Void = { _ in }
) -> OnboardingFlow {
    OnboardingFlow(
        steps: core.onboarding(), apps: [imovie, photos], sites: ["figma.com"],
        rules: { core.settings().rules }, accessibilityGranted: { true }, askForAccessibility: {},
        choose: { command in
            sent.commands.append(command)
            _ = core.handle(event: .commandChosen(command: command))
        },
        notifications: notifications, askForNotifications: {}, onFinish: onFinish)
}

/// Onboarding's flow on a staged core: what each user is asked, and what reaches the core when.
struct OnboardingFlowTests {
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
        let flow = onboarding(try core(bundleWritable: false), notifications: { .granted })

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
        let rules = staged.settings().rules
        #expect(rules.first { $0.identity == .app(books) }?.says == .on)
        #expect(rules.first { $0.identity == .app(tv) }?.says == .on)
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
        var finished: [Bool] = []
        let flow = onboarding(try core(), sent: sent, onFinish: { finished.append($0) })
        flow.go(to: try #require(flow.steps.firstIndex(of: .websites)))

        flow.perform(.turnOnWebsites)
        #expect(flow.page == .sites)
        #expect(finished == [])

        flow.finish()
        #expect(finished == [true])
        #expect(sent.commands == [])
    }

    @Test func notNowLeavesWebsiteCheckingOff() throws {
        var finished: [Bool] = []
        let flow = onboarding(try core(), onFinish: { finished.append($0) })
        flow.go(to: try #require(flow.steps.firstIndex(of: .websites)))

        flow.perform(.notNow)
        flow.finish()

        #expect(finished == [false])
    }

    @Test func allowingNotificationsWhereThereIsNoLoginItemMovesOnToReady() throws {
        var permission = NotificationPermission.notRequested
        let flow = onboarding(try core(bundleWritable: false), notifications: { permission })
        flow.go(to: try #require(flow.steps.firstIndex(of: .lastThings)))
        #expect(flow.step == .lastThings)

        permission = .granted

        #expect(!flow.steps.contains(.lastThings))
        #expect(flow.index == flow.steps.count - 1)
        #expect(flow.step == .ready)
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
