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
    case showOnboarding
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
    let mac: Mac
    let inbox: Inbox
    let executor: Executor

    /// Night Shift is unavailable unless `nightShift`.
    init(_ core: Core, nightShift: Bool = true) {
        let mac = Mac()
        let inbox = Inbox()
        self.mac = mac
        self.inbox = inbox
        executor = Executor(
            core: core, inbox: inbox, nightShift: nightShift ? mac : nil, loginItem: mac, updater: mac, wakeups: mac,
            browser: mac, accessibility: mac, notifications: mac, systemSettings: mac, rereading: mac,
            showOnboarding: { mac.record(.showOnboarding) })
        executor.onChange { mac.record(.redraw) }
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
    Outcome(effects: effects, events: [])
}

private let sunrise = at("2026-10-17T07:00:00+02:00")

/// How the executor carries out the core's outcomes: each effect by its adapter and each event by
/// whoever subscribed, in the core's order, on the main queue, and the release as the app quits.
struct ExecutorTests {
    @Test func eachEffectReachesItsAdapterInTheOrderTheCoreGaveThem() async throws {
        let shell = FakeShell(try onByTheSchedule())

        shell.inbox.outcome(
            outcome: outcome(
                .watchBrowser(bundleId: "com.apple.Safari", generation: 1), .setNightShift(enabled: false),
                .setSchedule(schedule: customHours), .setStrength(value: 40, commit: false),
                .scheduleWakeup(at: sunrise), .cancelWakeup, .stopWatching, .registerLoginItem,
                .unregisterLoginItem, .startUpdater, .checkForUpdates, .askForAccessibility,
                .showOnboarding(completed: 0)))
        await mainQueueTurns()

        #expect(
            shell.mac.calls == [
                .watch("com.apple.Safari", generation: 1), .setNightShift(false), .setSchedule(customHours),
                .setStrength(40, commit: false), .scheduleWakeup(sunrise), .cancelWakeup, .stopWatching,
                .registerLoginItem, .rereadLoginItem, .unregisterLoginItem, .rereadLoginItem, .startUpdater,
                .checkForUpdates, .askForAccessibility, .showOnboarding, .redraw,
            ])
    }

    @Test func aSavedStrengthIsReadBackAndAPreviewIsnt() async throws {
        let shell = FakeShell(try onByTheSchedule())

        shell.inbox.outcome(
            outcome: outcome(.setStrength(value: 40, commit: false), .setStrength(value: 60, commit: true)))
        await mainQueueTurns()

        #expect(
            shell.mac.calls == [
                .setStrength(40, commit: false), .setStrength(60, commit: true), .rereadStrength, .redraw,
            ])
    }

    @Test func theLoginItemIsReadBackEvenWhenMacOSRefusesTheChange() async throws {
        let shell = FakeShell(try onByTheSchedule())
        shell.mac.refusesLoginItem = true

        shell.inbox.outcome(outcome: outcome(.registerLoginItem, .unregisterLoginItem))
        await mainQueueTurns()

        #expect(
            shell.mac.calls == [
                .registerLoginItem, .rereadLoginItem, .unregisterLoginItem, .rereadLoginItem, .redraw,
            ])
    }

    @Test func whileNightShiftIsUnavailableItsEffectsAreDroppedAndTheRestCarriedOut() async throws {
        let shell = FakeShell(try onByTheSchedule(), nightShift: false)

        shell.inbox.outcome(
            outcome: outcome(
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
        let updated = Event.updated(from: "1.1.0", to: "1.2.0")

        shell.inbox.outcome(outcome: Outcome(effects: [.setNightShift(enabled: false)], events: [refused, updated]))
        shell.inbox.outcome(outcome: outcome())
        await mainQueueTurns()

        #expect(
            mac.calls == [
                .setNightShift(false), .told(refused, subscriber: 1), .told(refused, subscriber: 2),
                .told(updated, subscriber: 1), .told(updated, subscriber: 2), .redraw, .nextRedraw, .redraw,
            ])
    }

    @Test func outcomesFromTheCoresThreadAreCarriedOutOnTheMainQueueInTheOrderTold() async throws {
        let shell = FakeShell(try onByTheSchedule())
        let inbox = shell.inbox
        let strengths = Array(UInt8(0)...100)

        await withCheckedContinuation { done in
            var left = strengths.count
            shell.executor.onChange {
                left -= 1
                if left == 0 { done.resume() }
            }
            Thread.detachNewThread {
                for strength in strengths {
                    inbox.outcome(outcome: outcome(.setStrength(value: strength, commit: false)))
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
        shell.inbox.outcome(outcome: outcome(.watchBrowser(bundleId: "com.apple.Safari", generation: 1)))

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
