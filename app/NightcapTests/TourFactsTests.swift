import Foundation
import NightcapCore
import Testing

private let site = "nightcap.choco.me"

private let away = TourPlace(inFront: false, rule: nil, snoozed: false)

/// What the tour's steps follow, from `core`'s answers, whose switch is always the Night Shift
/// item's.
private func facts(_ core: Core, site: String? = site) throws -> TourFacts {
    let facts = try #require(TourFacts(core.snapshot(), sample: tourSample, site: site))
    #expect(facts.everywhereOn == core.menu().nightShift.on)
    return facts
}

/// What a running tour's steps follow, as the core's answers have it (ADR-0016).
struct TourFactsTests {
    @Test func noTourRunsNoFacts() throws {
        let core = try onByTheSchedule()

        #expect(TourFacts(core.snapshot(), sample: tourSample, site: site) == nil)
    }

    @Test func theTourAsksForTheStandInWhereItFindsNightShiftOffAndItsFactsSayItStandsIn() throws {
        let off = try offByTheSchedule()
        let on = try onByTheSchedule()
        #expect(TourFacts.asksForStandIn(off.snapshot()))
        #expect(!TourFacts.asksForStandIn(on.snapshot()))

        for core in [off, on] {
            let asked = TourFacts.asksForStandIn(core.snapshot())
            core.sendApp(command: .beginTrial(standsIn: asked))

            #expect(try facts(core).standsIn == asked)
        }
    }

    @Test func theSamplesFactsFollowItToTheFrontIntoItsRuleAndAway() throws {
        let core = try onByTheSchedule(tourBegun)
        #expect(try facts(core).sample == away)

        core.bringToFront(tourSample)
        #expect(try facts(core).sample == TourPlace(inFront: true, rule: nil, snoozed: false))
        core.send(command: try #require(core.appRow).choice(titled: "Always").command)
        #expect(try facts(core).sample == TourPlace(inFront: true, rule: .off, snoozed: false))
        core.bringToFront("com.apple.mail")

        #expect(try facts(core).sample == TourPlace(inFront: false, rule: .off, snoozed: false))
    }

    @Test func aSnoozeFromTheSamplesRowAndPuttingTheRuleBackShowInItsFacts() throws {
        let core = try onByTheSchedule(
            tourBegun, front(tourSample), .command(.addRule(identity: .app(tourSample), says: .off)))

        core.send(command: try #require(core.appRow).choice(titled: "For 30 Minutes").command)
        #expect(try facts(core).sample == TourPlace(inFront: true, rule: .off, snoozed: true))
        core.send(command: try #require(core.appRow).choice(titled: "Put the Rule Back").command)

        #expect(try facts(core).sample == TourPlace(inFront: true, rule: .off, snoozed: false))
    }

    @Test func theSamplesRuleIsSnoozedWhileItsShortestRuleSaysTheOtherWay() throws {
        let core = try onByTheSchedule(
            tourBegun, front(tourSample), .command(.addRule(identity: .app(tourSample), says: .off)))
        let sample = Place.app(tourSample)

        core.send(command: .set(place: sample, state: .on, length: .untilScheduleTurns))
        #expect(try facts(core).sample == TourPlace(inFront: true, rule: .off, snoozed: true))
        core.send(command: .set(place: sample, state: .off, length: minutes(30)))

        #expect(try facts(core).sample == TourPlace(inFront: true, rule: .off, snoozed: false))
    }

    @Test func anOutsideChangeSnoozesTheSamplesRuleInItsFacts() throws {
        let core = try onByTheSchedule(
            tourBegun, front(tourSample), .command(.addRule(identity: .app(tourSample), says: .off)),
            written(false, at: "2026-10-16T23:00:00+02:00"))

        core.report(
            fact: .statusChanged(
                enabled: true, schedule: customHours, sunScheduleSetAside: false,
                writtenAt: at("2026-10-16T23:00:30+02:00"), switchedBy: .outside))

        #expect(try facts(core).sample == TourPlace(inFront: true, rule: .off, snoozed: true))
    }

    @Test func theSitesOwnRuleShowsInItsFactsAndATapChangesOnlyTheSwitch() throws {
        let core = try onByTheSchedule(
            config: "websites = true\n", tourBegun, front("com.apple.Safari", browser: true),
            host("www.nightcap.choco.me"))
        let always = try #require(core.siteRow).choice(titled: "Always")
        #expect(always.command == .addRule(identity: .domain(site), says: .off))
        core.send(command: always.command)
        let ruled = try facts(core)
        #expect(ruled.site == TourPlace(inFront: true, rule: .off, snoozed: false))
        #expect(ruled.everywhereOn)

        core.send(command: .toggle)

        #expect(
            try facts(core)
                == TourFacts(standsIn: ruled.standsIn, everywhereOn: false, sample: ruled.sample, site: ruled.site))
    }

    @Test func aTourWithoutASiteStepFollowsNoSite() throws {
        #expect(try facts(try onByTheSchedule(tourBegun), site: nil).site == nil)
    }
}
