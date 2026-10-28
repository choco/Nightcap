import Foundation
import NightcapCore
import Testing

/// What the executor had macOS do, or told whoever subscribed, in order.
private enum Call: Equatable {
    case setNightShift(Bool)
    case setSchedule(Schedule)
    case setStrength(UInt8, commit: Bool)
    case registerLoginItem
    case unregisterLoginItem
    case rereadLoginItem
    case rereadStrength
    case startUpdater
    case checkForUpdates
    case scheduleWakeup(Date)
    case cancelWakeup
    case watch(String, generation: UInt64)
    case stopWatching
    case askForAccessibility
    case allowNotifications
    case open(SystemSettingsPane)
    case told(Event, subscriber: Int)
    case redraw
    case nextRedraw
}

private struct Refused: Error {}

/// macOS as the executor drives it, through every adapter and the reporter's rereads, recording
/// each call and whether it came on the main thread.
private final class Mac: NightShiftWriting, LoginItemRegistering, Updating, WakeupScheduling, BrowserWatching,
    AccessibilityAsking, NotificationsAsking, SystemSettingsOpening, Rereading
{
    private(set) var calls: [Call] = []
    private(set) var onMainThread: [Bool] = []
    var host: String?
    /// Whether macOS refuses to register or unregister the login item.
    var refusesLoginItem = false

    func record(_ call: Call) {
        calls.append(call)
        onMainThread.append(Thread.isMainThread)
    }

    func setEnabled(_ enabled: Bool) { record(.setNightShift(enabled)) }
    func setSchedule(_ schedule: Schedule) { record(.setSchedule(schedule)) }
    func setStrength(_ value: UInt8, commit: Bool) { record(.setStrength(value, commit: commit)) }

    func register() throws {
        record(.registerLoginItem)
        if refusesLoginItem { throw Refused() }
    }

    func unregister() throws {
        record(.unregisterLoginItem)
        if refusesLoginItem { throw Refused() }
    }

    func rereadLoginItem() { record(.rereadLoginItem) }
    func rereadStrength() { record(.rereadStrength) }
    func start() { record(.startUpdater) }
    func checkForUpdates() { record(.checkForUpdates) }
    func schedule(at date: Date) { record(.scheduleWakeup(date)) }
    func cancel() { record(.cancelWakeup) }
    func watch(bundleId: String, generation: UInt64) { record(.watch(bundleId, generation: generation)) }
    func stop() { record(.stopWatching) }
    func ask() { record(.askForAccessibility) }
    func allowNotifications() { record(.allowNotifications) }
    func open(_ pane: SystemSettingsPane) { record(.open(pane)) }
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
            browser: mac, accessibility: mac, notifications: mac, systemSettings: mac, rereading: mac)
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
    Outcome(cause: .fact, effects: effects, events: [])
}

private let sunrise = at("2026-10-17T07:00:00+02:00")

/// How the executor carries out the core's outcomes: each effect by its adapter and each event by
/// whoever subscribed, in the core's order, on the main queue, and the release as the app quits.
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
                .registerLoginItem, .rereadLoginItem, .unregisterLoginItem, .rereadLoginItem, .startUpdater,
                .checkForUpdates, .redraw,
            ])
    }

    @Test func aSavedStrengthIsReadBackAndAPreviewIsnt() async throws {
        let shell = FakeShell(try onByTheSchedule())

        shell.tell(
            outcome(.setStrength(value: 40, commit: false), .setStrength(value: 60, commit: true)))
        await mainQueueTurns()

        #expect(
            shell.mac.calls == [
                .setStrength(40, commit: false), .setStrength(60, commit: true), .rereadStrength, .redraw,
            ])
    }

    @Test func theLoginItemIsReadBackEvenWhenMacOSRefusesTheChange() async throws {
        let shell = FakeShell(try onByTheSchedule())
        shell.mac.refusesLoginItem = true

        shell.tell(outcome(.registerLoginItem, .unregisterLoginItem))
        await mainQueueTurns()

        #expect(
            shell.mac.calls == [
                .registerLoginItem, .rereadLoginItem, .unregisterLoginItem, .rereadLoginItem, .redraw,
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

    @Test func eventsReachEverySubscriberAfterTheEffectsAndBeforeTheRedraw() async throws {
        let shell = FakeShell(try onByTheSchedule())
        let mac = shell.mac
        shell.executor.onEvent { mac.record(.told($0, subscriber: 1)) }
        shell.executor.onEvent { mac.record(.told($0, subscriber: 2)) }
        shell.executor.onNextChange { mac.record(.nextRedraw) }
        let refused = Event.refused(command: .toggle, why: .nightShiftUnavailable)
        let adopted = Event.adopted(place: .everywhere, snoozed: nil, state: .off, until: sunrise)

        shell.tell(Outcome(cause: .fact, effects: [.setNightShift(enabled: false)], events: [refused, adopted]))
        shell.tell(outcome())
        await mainQueueTurns()

        #expect(
            mac.calls == [
                .setNightShift(false), .told(refused, subscriber: 1), .told(refused, subscriber: 2),
                .told(adopted, subscriber: 1), .told(adopted, subscriber: 2), .redraw, .nextRedraw, .redraw,
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

        shell.inbox.outcome(outcome: Outcome(cause: .launch, effects: [], events: []), snapshot: launched)
        shell.inbox.outcome(outcome: Outcome(cause: .command, effects: [], events: []), snapshot: switchedOff)
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

        shell.inbox.outcome(outcome: Outcome(cause: .command, effects: [], events: []), snapshot: turnedOn)
        shell.inbox.outcome(outcome: Outcome(cause: .command, effects: [], events: []), snapshot: turnedOff)
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
                accessibilityGranted, reported(true), front("com.figma.Desktop"), reported(false),
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
