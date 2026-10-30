import NightcapCore
import Testing

struct TourFlowTests {
    @Test func continuingWaitsForTheTrialBeforeOpeningTheSample() throws {
        let core = try offByTheSchedule()
        var flow = TourFlow(sample: tourSample, site: nil, hasShortcut: true)
        #expect(flow.refresh(core.snapshot()).isEmpty)
        #expect(flow.standsIn)
        #expect(flow.advance() == [.beginTrial(standsIn: true)])
        #expect(!flow.canAdvance)
        #expect(flow.advance().isEmpty)
        #expect(flow.refresh(core.snapshot()).isEmpty)
        #expect(flow.step == .moon)
        #expect(!flow.usesSample)

        core.sendApp(command: .beginTrial(standsIn: true))
        #expect(flow.refresh(core.snapshot()) == [.openSample])
        #expect(flow.step == .rule)
        #expect(flow.canAdvance)
        #expect(flow.refresh(core.snapshot()).isEmpty)
    }

    @Test func endingWhileTheTrialBeginsEndsItOnceAndIgnoresItsLateSnapshot() throws {
        let core = try onByTheSchedule()
        var flow = TourFlow(sample: tourSample, site: nil, hasShortcut: true)
        _ = flow.refresh(core.snapshot())
        #expect(flow.advance() == [.beginTrial(standsIn: false)])
        #expect(flow.end() == [.closeSample, .finish(begun: true)])
        core.sendApp(command: .beginTrial(standsIn: false))
        #expect(flow.refresh(core.snapshot()).isEmpty)
        #expect(flow.advance().isEmpty)
        #expect(flow.end().isEmpty)
        #expect(!flow.usesSample)
    }

    @Test func endingTheIntroductionDoesNotEndATrial() {
        var flow = TourFlow(sample: tourSample, site: nil, hasShortcut: false)
        #expect(flow.end() == [.closeSample, .finish(begun: false)])
        #expect(flow.end().isEmpty)
    }

    @Test func skippingTheRuleSkipsSnoozingAndOnlyVisitsAvailableSteps() throws {
        for hasSite in [false, true] {
            for hasShortcut in [false, true] {
                let core = try onByTheSchedule()
                var flow = TourFlow(
                    sample: tourSample, site: hasSite ? "nightcap.choco.me" : nil, hasShortcut: hasShortcut)
                _ = flow.advance()
                core.sendApp(command: .beginTrial(standsIn: false))
                _ = flow.refresh(core.snapshot())
                #expect(flow.advance() == (hasSite ? [.closeSample, .openSite] : [.closeSample]))
                if hasSite {
                    #expect(flow.step == .site)
                    #expect(flow.advance().isEmpty)
                }
                if hasShortcut {
                    #expect(flow.step == .peek)
                    #expect(!flow.done)
                    flow.shortcutPressed()
                    #expect(flow.keysDown)
                    flow.tried(.hold)
                    #expect(!flow.keysDown)
                    #expect(flow.done)
                    #expect(flow.lastTry == .hold)
                    #expect(flow.advance().isEmpty)
                }
                #expect(flow.step == .end)
                #expect(!flow.canAdvance)
                #expect(flow.advance().isEmpty)
                #expect(flow.end() == [.closeSample, .finish(begun: true)])
            }
        }
    }

    @Test func theRuleAndSnoozeStepsFollowChangesMadeThroughTheCore() throws {
        let core = try onByTheSchedule()
        var flow = TourFlow(sample: tourSample, site: nil, hasShortcut: false)
        _ = flow.advance()
        core.sendApp(command: .beginTrial(standsIn: false))
        _ = flow.refresh(core.snapshot())
        core.bringToFront(tourSample)
        core.send(command: .addRule(identity: .app(tourSample), says: .off))
        _ = flow.refresh(core.snapshot())
        #expect(!flow.done)
        core.bringToFront("com.apple.mail")
        _ = flow.refresh(core.snapshot())
        #expect(flow.done)
        #expect(flow.advance().isEmpty)
        #expect(flow.step == .snooze)
        #expect(flow.usesSample)
        #expect(!flow.done)
        core.send(command: .set(place: .app(tourSample), state: .on, length: minutes(30)))
        _ = flow.refresh(core.snapshot())
        core.send(command: .reset(.app(tourSample)))
        _ = flow.refresh(core.snapshot())
        #expect(flow.done)
        #expect(flow.advance() == [.closeSample])
        #expect(flow.step == .end)
    }

    @Test func aTrialEndingClosesTheTour() throws {
        let core = try onByTheSchedule()
        var flow = TourFlow(sample: tourSample, site: nil, hasShortcut: false)
        _ = flow.advance()
        core.sendApp(command: .beginTrial(standsIn: false))
        _ = flow.refresh(core.snapshot())
        core.sendApp(command: .endTrial)
        #expect(flow.refresh(core.snapshot()) == [.closeSample, .finish(begun: true)])
        #expect(flow.refresh(core.snapshot()).isEmpty)
    }
}
