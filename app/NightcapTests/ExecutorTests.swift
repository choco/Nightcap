import Foundation
import NightcapCore
import Testing

/// What the executor had macOS do, told the user, or ran on a reply, in order.
private enum Call: Equatable {
    case setNightShift(Bool)
    case setSchedule(Schedule)
    case setStrength(UInt8, commit: Bool)
    case registerLoginItem
    case unregisterLoginItem
    case startUpdater
    case checkForUpdates
    case scheduleWakeup(Date)
    case cancelWakeup
    case watch(String, generation: UInt64)
    case stopWatching
    case askForAccessibility
    case allowNotifications
    case open(SystemSettingsPane)
    case toldAdopted(snoozed: RuleIdentity?, state: OnOff)
    case toldRefused(Command, Refusal)
    case replied(Reply)
    case redraw
}

/// macOS as the executor drives it through its adapters, recording
/// each call and whether it came on the main thread.
private final class Mac: NightShiftWriting, LoginItemRegistering, Updating, WakeupScheduling, BrowserWatching,
    AccessibilityAsking, NotificationsAsking, SystemSettingsOpening, Telling
{
    private(set) var calls: [Call] = []
    private(set) var onMainThread: [Bool] = []
    var host: String?

    func record(_ call: Call) {
        calls.append(call)
        onMainThread.append(Thread.isMainThread)
    }

    func setEnabled(_ enabled: Bool) { record(.setNightShift(enabled)) }
    func setSchedule(_ schedule: Schedule) { record(.setSchedule(schedule)) }
    func setStrength(_ value: UInt8, commit: Bool) { record(.setStrength(value, commit: commit)) }

    func register() { record(.registerLoginItem) }
    func unregister() { record(.unregisterLoginItem) }
    func start() { record(.startUpdater) }
    func checkForUpdates() { record(.checkForUpdates) }
    func schedule(at date: Date) { record(.scheduleWakeup(date)) }
    func cancel() { record(.cancelWakeup) }
    func watch(bundleId: String, generation: UInt64) { record(.watch(bundleId, generation: generation)) }
    func stop() { record(.stopWatching) }
    func ask() { record(.askForAccessibility) }
    func allowNotifications() { record(.allowNotifications) }
    func open(_ pane: SystemSettingsPane) { record(.open(pane)) }
    func tell(adoptedSnoozing snoozed: RuleIdentity?, state: OnOff, until: Date?) {
        record(.toldAdopted(snoozed: snoozed, state: state))
    }
    func tell(refused command: Command, why: Refusal) { record(.toldRefused(command, why)) }
}

/// An executor carrying out what a staged core decides on a fake macOS, which records each redraw
/// too, with the inbox the core would tell. The inbox holds the executor weakly, as the app holds
/// it, so this does.
private struct FakeShell {
    let core: Core
    let mac: Mac
    let inbox: Inbox
    let executor: Executor

    /// Night Shift is unavailable unless `nightShift`.
    init(_ core: Core, nightShift: Bool = true) {
        let mac = Mac()
        let inbox = Inbox()
        self.core = core
        self.mac = mac
        self.inbox = inbox
        executor = Executor(
            core: core, inbox: inbox, nightShift: nightShift ? mac : nil, loginItem: mac, updater: mac, wakeups: mac,
            browser: mac, accessibility: mac, notifications: mac, systemSettings: mac, telling: mac)
        executor.onChange { _, _ in mac.record(.redraw) }
    }

    /// Tells the inbox `outcome`, with the snapshot the core last published, as the core would.
    func tell(_ outcome: Outcome) {
        inbox.outcome(outcome: outcome, snapshot: core.snapshot())
    }
}

/// Lets the main queue run what's already on it, as the inbox's hand-overs are.
private func mainQueueTurns() async {
    await withCheckedContinuation { done in
        DispatchQueue.main.async { done.resume() }
    }
}

/// An outcome of `effects` alone.
nonisolated private func outcome(_ effects: Effect...) -> Outcome {
    Outcome(cause: .fact, effects: effects, reply: nil)
}

private let sunrise = at("2026-10-17T07:00:00+02:00")

/// How the executor carries out the core's outcomes: each effect by its adapter, in the core's
/// order, on the main queue, then what waits on a command's reply, and the release as the app
/// quits.
struct ExecutorTests {
    @Test func eachEffectReachesItsAdapterInTheOrderTheCoreGaveThem() async throws {
        let shell = FakeShell(try onByTheSchedule())

        shell.tell(
            outcome(
                .watchBrowser(bundleId: "com.apple.Safari", generation: 1), .setNightShift(enabled: false),
                .setSchedule(schedule: customHours), .setStrength(value: 40, commit: false),
                .scheduleWakeup(at: sunrise), .cancelWakeup, .stopWatching, .registerLoginItem,
                .unregisterLoginItem, .startUpdater, .checkForUpdates))
        await mainQueueTurns()

        #expect(
            shell.mac.calls == [
                .watch("com.apple.Safari", generation: 1), .setNightShift(false), .setSchedule(customHours),
                .setStrength(40, commit: false), .scheduleWakeup(sunrise), .cancelWakeup, .stopWatching,
                .registerLoginItem, .unregisterLoginItem, .startUpdater,
                .checkForUpdates, .redraw,
            ])
    }

    @Test func whileNightShiftIsUnavailableItsEffectsAreDroppedAndTheRestCarriedOut() async throws {
        let shell = FakeShell(try onByTheSchedule(), nightShift: false)

        shell.tell(
            outcome(
                .setNightShift(enabled: false), .setSchedule(schedule: customHours),
                .setStrength(value: 60, commit: true), .scheduleWakeup(at: sunrise)))
        await mainQueueTurns()

        #expect(shell.mac.calls == [.scheduleWakeup(sunrise), .redraw])
    }

    /// The peek's case (ADR-0015): what waits on a command runs once its own outcome is carried
    /// out, after its effects and the redraw, never on an outcome told before it.
    @Test func aCommandsReplyRunsWhatWaitsOnItAfterItsOwnOutcome() async throws {
        let shell = FakeShell(try onByTheSchedule())
        let mac = shell.mac
        shell.executor.send(.toggle) { mac.record(.replied($0)) }
        let done = Reply.done(.updateCheckStarted)

        shell.tell(outcome(.stopWatching))
        shell.tell(
            Outcome(cause: .command, effects: [.setNightShift(enabled: false)], reply: Replied(ticket: 1, reply: done)))
        await mainQueueTurns()

        #expect(mac.calls == [.stopWatching, .redraw, .setNightShift(false), .redraw, .replied(done)])
    }

    @Test func aRefusalIsToldBeforeTheRedrawAndAnAdoptionInItsPlaceAmongTheEffects() async throws {
        let shell = FakeShell(try onByTheSchedule())
        let mac = shell.mac
        shell.executor.send(.on)
        let refused = Reply.refused(.nightShiftUnavailable)

        shell.tell(
            Outcome(
                cause: .fact,
                effects: [
                    .adopted(place: .everywhere, snoozed: nil, state: .off, until: sunrise),
                    .setNightShift(enabled: false),
                ], reply: nil))
        shell.tell(Outcome(cause: .command, effects: [], reply: Replied(ticket: 1, reply: refused)))
        await mainQueueTurns()

        #expect(
            mac.calls == [
                .toldAdopted(snoozed: nil, state: .off), .setNightShift(false), .redraw,
                .toldRefused(.on, .nightShiftUnavailable), .redraw,
            ])
    }

    @Test func eachChangeSaysWhatItsOutcomeAnswersWithTheSnapshotItsInputLeft() async throws {
        let core = try onByTheSchedule()
        let shell = FakeShell(core)
        let launched = core.snapshot()
        core.send(command: .off)
        let switchedOff = core.snapshot()
        var heard: [(cause: Cause, snapshot: Snapshot)] = []
        shell.executor.onChange { heard.append(($0, $1)) }

        shell.inbox.outcome(outcome: Outcome(cause: .launch, effects: [], reply: nil), snapshot: launched)
        shell.inbox.outcome(outcome: Outcome(cause: .command, effects: [], reply: nil), snapshot: switchedOff)
        await mainQueueTurns()
        shell.executor.stop()

        #expect(heard.map(\.cause) == [.launch, .command, .quit])
        #expect(heard[0].snapshot === launched)
        #expect(heard[1].snapshot === switchedOff)
        #expect(heard.prefix(2).map { $0.snapshot.front().everywhere.gives } == [.on, .off])
    }

    /// The redraw asks as AppDelegate's does, from each change's snapshot: the latest alone, with
    /// website checking off again, would hide the turn on.
    @Test func aTurnOnAndATurnOffQueuedTogetherStillAskForAccessibilityOnceForTheOn() async throws {
        let core = try atNoon(permissions(false, .granted))
        let shell = FakeShell(core)
        let executor = shell.executor
        var ask = AccessibilityAsk(core.snapshot())
        executor.onChange { _, snapshot in
            if ask.isDue(after: snapshot) { executor.perform(.askForAccessibility) }
        }
        core.send(command: .websites(.on))
        let turnedOn = core.snapshot()
        core.send(command: .websites(.off))
        let turnedOff = core.snapshot()

        shell.inbox.outcome(outcome: Outcome(cause: .command, effects: [], reply: nil), snapshot: turnedOn)
        shell.inbox.outcome(outcome: Outcome(cause: .command, effects: [], reply: nil), snapshot: turnedOff)
        await mainQueueTurns()

        #expect(shell.mac.calls == [.redraw, .askForAccessibility, .redraw])
    }

    @Test func outcomesFromTheCoresThreadAreCarriedOutOnTheMainQueueInTheOrderTold() async throws {
        let shell = FakeShell(try onByTheSchedule())
        let inbox = shell.inbox
        let snapshot = shell.core.snapshot()
        let strengths = Array(UInt8(0)...100)

        await withCheckedContinuation { done in
            var left = strengths.count
            shell.executor.onChange { _, _ in
                left -= 1
                if left == 0 { done.resume() }
            }
            Thread.detachNewThread {
                for strength in strengths {
                    inbox.outcome(outcome: outcome(.setStrength(value: strength, commit: false)), snapshot: snapshot)
                }
            }
        }

        #expect(shell.mac.calls.filter { $0 != .redraw } == strengths.map { .setStrength($0, commit: false) })
        #expect(shell.mac.onMainThread.allSatisfy { $0 })
    }

    /// A config rule keeps Figma off, and Safari came to the front with website checking on, so
    /// the core is watching it, its host not yet read, as the user quits (ADR-0008, ADR-0012).
    @Test func quittingCarriesOutTheReleaseAfterWhatWasStillOnItsWay() async throws {
        let shell = FakeShell(
            try staged(
                "2026-10-16T23:00:00+02:00",
                config: "websites = true\n[rules.off]\napp = [\"com.figma.Desktop\"]\n",
                accessibilityGranted, reported(true), front("com.figma.Desktop"),
                written(false, at: "2026-10-16T23:00:00+02:00"),
                front("com.apple.Safari", browser: true)))
        shell.tell(outcome(.watchBrowser(bundleId: "com.apple.Safari", generation: 1)))

        shell.executor.stop()

        let carriedOut: [Call] = [
            .watch("com.apple.Safari", generation: 1), .redraw, .stopWatching, .setNightShift(true), .redraw,
        ]
        #expect(shell.mac.calls == carriedOut)
        await mainQueueTurns()
        #expect(shell.mac.calls == carriedOut)
    }

    @Test func theAppsOwnFixesReachTheirAdapters() throws {
        let shell = FakeShell(try onByTheSchedule())

        for fix in [AppFix.askForAccessibility, .allowNotifications, .openLoginItems, .openLocationServices] {
            shell.executor.perform(fix)
        }

        #expect(
            shell.mac.calls == [
                .askForAccessibility, .allowNotifications, .open(.loginItems), .open(.locationServices),
            ])
    }
}
