import Foundation
import NightcapCore
import Testing

/// A config that answers website checking and both of Night Shift's steps, but not the apps.
private let answersAllButTheApps = """
    websites = false
    [night_shift]
    strength = 40
    schedule = "off"
    """

/// A config rule, which answers the apps.
private let aRule = """

    [rules.off]
    app = ["com.figma.Desktop"]
    """

/// A config that answers website checking, both of Night Shift's steps and, with a rule, the apps.
private let answersAll = answersAllButTheApps + aRule

/// The snapshot a managed release's launch leaves at noon, or a self-managed release's where
/// `selfManaged`, with `config` as its config, after macOS reported Night Shift off with no
/// schedule at a strength of 60 where `reported`.
private func launch(config: String? = nil, selfManaged: Bool = false, reported: Bool = true) throws -> Snapshot {
    let nightShift: [Step] = [
        .fact(.statusChanged(enabled: false, schedule: .off, sunScheduleSetAside: false, writtenAt: nil)),
        .fact(.strengthChanged(value: 60)),
    ]
    return try Core.staged(
        scenario: Scenario(
            now: at("2026-10-16T12:00:00+02:00"), bundleWritable: selfManaged, config: config,
            steps: (reported ? nightShift : []) + [.fact(.launched)])
    ).snapshot()
}

/// Whether onboarding opens as the launch's outcome is carried out, and what the app records of
/// it in its own preferences.
struct OnboardingRecordTests {
    @Test func onboardingOpensAtTheFirstLaunchAndNotOnceTheUserWentThroughIt() throws {
        let snapshot = try launch()
        withScratchDefaults { defaults in
            let record = OnboardingRecord(defaults: defaults)
            #expect(!record.finished)
            #expect(record.launched(snapshot))
            #expect(!record.finished)

            record.finish()

            #expect(record.finished)
            #expect(record.completed == 1)
            #expect(!OnboardingRecord(defaults: defaults).launched(snapshot))
        }
    }

    /// Going through it then counts as having done so.
    @Test func aConfigThatSetsWebsiteCheckingAndNightShiftWithARuleLeavesOnboardingNothingToAsk() throws {
        let snapshot = try launch(config: answersAll)
        withScratchDefaults { defaults in
            let record = OnboardingRecord(defaults: defaults)

            #expect(!record.launched(snapshot))
            #expect(record.finished)
        }
    }

    /// The user would otherwise start with no rule and no suggestion.
    @Test func aConfigThatSetsWebsiteCheckingAndNightShiftWithNoRuleStillLeavesTheAppsToAsk() throws {
        let snapshot = try launch(config: answersAllButTheApps)
        withScratchDefaults { defaults in
            let record = OnboardingRecord(defaults: defaults)

            #expect(record.launched(snapshot))
            #expect(!record.finished)
        }
    }

    /// Any rule of the config's answers the apps, a site's among them.
    @Test func aConfigRuleForASiteAnswersTheAppsToo() throws {
        let snapshot = try launch(config: answersAllButTheApps + "\n[rules.on]\ndomain = [\"youtube.com\"]\n")
        withScratchDefaults { defaults in
            #expect(!OnboardingRecord(defaults: defaults).launched(snapshot))
        }
    }

    @Test func aConfigThatSetsOnlyWebsiteCheckingStillLeavesNightShiftToAsk() throws {
        let snapshot = try launch(config: "websites = false\n" + aRule)
        withScratchDefaults { defaults in
            let record = OnboardingRecord(defaults: defaults)

            #expect(record.launched(snapshot))
            #expect(!record.finished)
        }
    }

    /// Without Night Shift's status the schedule can't be set, and without its strength the
    /// strength can't.
    @Test func aConfigThatSetsWebsiteCheckingWithARuleLeavesOnboardingNothingToAskWithoutNightShift() throws {
        let snapshot = try launch(config: "websites = false\n" + aRule, reported: false)
        withScratchDefaults { defaults in
            #expect(!OnboardingRecord(defaults: defaults).launched(snapshot))
        }
    }

    @Test func aSelfManagedReleaseStillAsksAboutOpeningAtLoginWhenTheConfigAnswersTheRest() throws {
        let snapshot = try launch(config: answersAll, selfManaged: true)
        withScratchDefaults { defaults in
            let record = OnboardingRecord(defaults: defaults)

            #expect(record.launched(snapshot))
            #expect(!record.finished)
        }
    }
}
