import Foundation
import NightcapCore
import ObjectiveC
import os

/// Drives Night Shift through the private CoreBrightness framework. Everything is looked up at
/// runtime and checked against the method signatures seen on macOS 15 and 27, so an OS update
/// that changes them leaves Night Shift unavailable instead of crashing (ADR-0004).
final class NightShiftAdapter: NightShiftWriting {
    struct Status {
        var enabled: Bool
        var schedule: Schedule
        /// Whether macOS has sunset to sunrise set aside, to put back once Location Services is on.
        var sunScheduleSetAside: Bool
        /// When Night Shift was last written, by Nightcap or another app such as Control Center:
        /// nil once the schedule has switched it since, or the schedule's mode changed (ADR-0004).
        var writtenAt: Date?
    }

    struct SunTimes {
        var sunrises: [Date]
        var sunsets: [Date]
    }

    private typealias GetStatus = @convention(c) (NSObject, Selector, UnsafeMutableRawPointer) -> Bool
    private typealias SetEnabled = @convention(c) (NSObject, Selector, Bool) -> Bool
    private typealias SetBlock = @convention(c) (NSObject, Selector, @escaping @convention(block) () -> Void) -> Void
    private typealias CopyProperty = @convention(c) (NSObject, Selector, NSString) -> Unmanaged<AnyObject>?
    private typealias GetStrength = @convention(c) (NSObject, Selector, UnsafeMutablePointer<Float>) -> Bool
    private typealias SetStrength = @convention(c) (NSObject, Selector, Float, Bool) -> Bool
    private typealias SetProperty = @convention(c) (NSObject, Selector, AnyObject, NSString) -> Bool
    private typealias SetMode = @convention(c) (NSObject, Selector, Int32) -> Bool
    private typealias SetSchedule = @convention(c) (NSObject, Selector, UnsafeRawPointer) -> Bool

    private static let frameworkPath = "/System/Library/PrivateFrameworks/CoreBrightness.framework/CoreBrightness"

    /// The status struct is {BOOL active, BOOL enabled, BOOL sunSchedulePermitted, int mode,
    /// {{int hour, int minute} from, {int hour, int minute} to} schedule, unsigned long long
    /// disableFlags, BOOL available}.
    private static let expectedEncodings = [
        "CBBlueLightClient": [
            "getBlueLightStatus:": "B24@0:8^{?=BBBi{?={?=ii}{?=ii}}QB}16",
            "setEnabled:": "B20@0:8B16",
            "setStatusNotificationBlock:": "v24@0:8@?16",
        ],
        "BrightnessSystemClient": [
            "copyPropertyForKey:": "@24@0:8@16"
        ],
    ]
    /// Checked apart from the rest: without them Night Shift still works, with no strength to
    /// show or set (ADR-0014).
    private static let strengthEncodings = [
        "getStrength:": "B24@0:8^f16",
        "setStrength:commit:": "B24@0:8f16B20",
    ]
    /// Checked apart from the rest too: without them, onboarding sends the user to System
    /// Settings to choose the schedule (ADR-0014). The custom hours are
    /// {{int hour, int minute} from, {int hour, int minute} to}.
    private static let scheduleEncodings = [
        "setMode:": "B20@0:8i16",
        "setSchedule:": "B24@0:8r^{?={?=ii}{?=ii}}16",
    ]
    /// The peek's setter, checked apart from the rest too: without it the shortcut only turns
    /// Night Shift on or off (ADR-0015).
    private static let peekEncodings = ["setProperty:forKey:": "B32@0:8@16@24"]
    private static let statusSize = 40
    private static let enabledOffset = 1
    private static let modeOffset = 4
    private static let fromHourOffset = 8
    private static let fromMinuteOffset = 12
    private static let toHourOffset = 16
    private static let toMinuteOffset = 20
    private static let availableOffset = 32

    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "nightshift")
    private let client: NSObject
    private let systemClient: NSObject
    private let getStatus: GetStatus
    private let setEnabledImplementation: SetEnabled
    private let copyProperty: CopyProperty
    private let strengthControl: NightShiftStrength?
    private let setMode: SetMode?
    private let setScheduleImplementation: SetSchedule?
    private let setProperty: SetProperty?
    /// Night Shift and its tint as the peek found them, while one lasts, which ending it puts
    /// back.
    private var peekedFrom: (enabled: Bool, factor: Float)?
    private var writes = NightShiftWrites()

    /// Fails when CoreBrightness or its expected methods are missing. `onChange` runs on the main
    /// thread with the new status, and whose write it's the first to show, whenever
    /// CoreBrightness reports a status change. `onStrength` reports a fresh strength read
    /// after each committed write.
    init?(
        onChange: @escaping @MainActor (Status, SwitchedBy) -> Void,
        onStrength: @escaping (UInt8) -> Void
    ) {
        let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "nightshift")
        guard dlopen(Self.frameworkPath, RTLD_NOW) != nil else {
            log.fault("CoreBrightness is missing")
            return nil
        }
        var classes: [String: NSObject.Type] = [:]
        for (className, methods) in Self.expectedEncodings {
            guard let found = NSClassFromString(className) as? NSObject.Type else {
                log.fault("\(className, privacy: .public) is missing")
                return nil
            }
            for (name, expected) in methods {
                let method = class_getInstanceMethod(found, NSSelectorFromString(name))
                let encoding = method.flatMap { method_getTypeEncoding($0) }.map { String(cString: $0) }
                guard encoding == expected else {
                    log.fault(
                        """
                        \(className, privacy: .public).\(name, privacy: .public) is \
                        \(encoding ?? "missing", privacy: .public), expected \(expected, privacy: .public)
                        """)
                    return nil
                }
            }
            classes[className] = found
        }
        client = classes["CBBlueLightClient"]!.init()
        systemClient = classes["BrightnessSystemClient"]!.init()
        getStatus = unsafeBitCast(client.method(for: NSSelectorFromString("getBlueLightStatus:")), to: GetStatus.self)
        setEnabledImplementation = unsafeBitCast(
            client.method(for: NSSelectorFromString("setEnabled:")), to: SetEnabled.self)
        copyProperty = unsafeBitCast(
            systemClient.method(for: NSSelectorFromString("copyPropertyForKey:")), to: CopyProperty.self)
        let found = { (encodings: [String: String], what: String, className: String) in
            encodings.allSatisfy { name, expected in
                let method = class_getInstanceMethod(classes[className]!, NSSelectorFromString(name))
                let encoding = method.flatMap { method_getTypeEncoding($0) }.map { String(cString: $0) }
                if encoding != expected {
                    log.fault(
                        """
                        \(className, privacy: .public).\(name, privacy: .public) is \
                        \(encoding ?? "missing", privacy: .public), expected \(expected, privacy: .public); \
                        \(what, privacy: .public) can't be set
                        """)
                }
                return encoding == expected
            }
        }
        let strengthFound = found(Self.strengthEncodings, "the strength", "CBBlueLightClient")
        let scheduleFound = found(Self.scheduleEncodings, "the schedule", "CBBlueLightClient")
        let peekFound = found(Self.peekEncodings, "the peek", "BrightnessSystemClient")
        if strengthFound {
            let get = unsafeBitCast(client.method(for: NSSelectorFromString("getStrength:")), to: GetStrength.self)
            let set = unsafeBitCast(
                client.method(for: NSSelectorFromString("setStrength:commit:")), to: SetStrength.self)
            let client = client
            strengthControl = NightShiftStrength(
                read: {
                    var value: Float = -1
                    return get(client, NSSelectorFromString("getStrength:"), &value) ? value : nil
                },
                write: { value, commit in set(client, NSSelectorFromString("setStrength:commit:"), value, commit) },
                onStrength: onStrength)
        } else {
            strengthControl = nil
        }
        setMode =
            scheduleFound
            ? unsafeBitCast(client.method(for: NSSelectorFromString("setMode:")), to: SetMode.self) : nil
        setScheduleImplementation =
            scheduleFound
            ? unsafeBitCast(client.method(for: NSSelectorFromString("setSchedule:")), to: SetSchedule.self) : nil
        setProperty =
            peekFound
            ? unsafeBitCast(systemClient.method(for: NSSelectorFromString("setProperty:forKey:")), to: SetProperty.self)
            : nil

        let setBlock = unsafeBitCast(
            client.method(for: NSSelectorFromString("setStatusNotificationBlock:")), to: SetBlock.self)
        // CoreBrightness calls this on one of its own queues, often several times per change.
        setBlock(client, NSSelectorFromString("setStatusNotificationBlock:")) { [weak self] in
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    guard let reported = self?.report() else { return }
                    onChange(reported.status, reported.switchedBy)
                }
            }
        }
    }

    /// Night Shift's status to report to the core, and whose write it's the first to show, or nil
    /// when CoreBrightness can't say. Every status Nightcap reports comes from here, so each
    /// write is told once.
    func report() -> (status: Status, switchedBy: SwitchedBy)? {
        guard let status else { return nil }
        return (status, writes.report(enabled: status.enabled, writtenAt: status.writtenAt))
    }

    /// Night Shift's state and schedule, or nil when CoreBrightness can't say.
    private var status: Status? {
        let raw = UnsafeMutableRawPointer.allocate(byteCount: Self.statusSize, alignment: 8)
        defer { raw.deallocate() }
        raw.initializeMemory(as: UInt8.self, repeating: 0, count: Self.statusSize)
        guard getStatus(client, NSSelectorFromString("getBlueLightStatus:"), raw),
            raw.load(fromByteOffset: Self.availableOffset, as: Bool.self)
        else {
            log.error("CoreBrightness didn't report a usable status")
            return nil
        }
        let int = { (offset: Int) in raw.load(fromByteOffset: offset, as: Int32.self) }
        let mode = int(Self.modeOffset)
        let schedule: Schedule
        switch mode {
        case 0: schedule = .off
        case 1: schedule = .sunsetToSunrise
        case 2:
            // CoreBrightness keeps the custom hours in every mode; only mode 2 uses them.
            guard let from = Self.timeOfDay(int(Self.fromHourOffset), int(Self.fromMinuteOffset)),
                let to = Self.timeOfDay(int(Self.toHourOffset), int(Self.toMinuteOffset))
            else {
                log.error("CoreBrightness reported custom hours out of range")
                return nil
            }
            schedule = .custom(from: from, to: to)
        default:
            log.error("CoreBrightness reported an unknown schedule mode \(mode)")
            return nil
        }
        guard let marks else { return nil }
        return Status(
            enabled: raw.load(fromByteOffset: Self.enabledOffset, as: Bool.self),
            schedule: schedule,
            sunScheduleSetAside: marks.sunScheduleSetAside,
            writtenAt: marks.writtenAt
        )
    }

    /// What the status struct leaves out, from the dictionary behind it.
    private struct Marks {
        /// The mark CoreBrightness keeps to go back to sunset to sunrise, set while Location
        /// Services is off.
        var sunScheduleSetAside: Bool
        var writtenAt: Date?
    }

    /// Nil when CoreBrightness marks no writes, without which Nightcap can't tell the schedule's
    /// switches from the user's (ADR-0004).
    private var marks: Marks? {
        let selector = NSSelectorFromString("copyPropertyForKey:")
        guard
            let status = copyProperty(systemClient, selector, "CBBlueReductionStatus")?
                .takeRetainedValue() as? [String: Any],
            let written = status["BlueLightReductionAlgoOverride"] as? NSNumber
        else {
            log.fault("CoreBrightness reported no mark of writes, so Night Shift can't be reconciled")
            return nil
        }
        let setAside = status["BlueLightReductionRevertToSunriseSunset"] as? Bool
        if setAside == nil {
            log.error("CoreBrightness reported no mark to go back to sunset to sunrise")
        }
        // 0 while no write stands; a write marks 3 for off or 4 for on, with its time.
        guard written.intValue != 0 else { return Marks(sunScheduleSetAside: setAside ?? false) }
        guard let writtenAt = status["BlueLightReductionAlgoOverrideTimestamp"] as? Date else {
            log.error("CoreBrightness reported a write with no time")
            return nil
        }
        return Marks(sunScheduleSetAside: setAside ?? false, writtenAt: writtenAt)
    }

    /// The sunrises and sunsets CoreBrightness schedules by, from the day before to the day
    /// after, or nil when it reports none.
    var sunTimes: SunTimes? {
        let selector = NSSelectorFromString("copyPropertyForKey:")
        guard
            let schedule = copyProperty(systemClient, selector, "BlueLightSunSchedule")?
                .takeRetainedValue() as? [String: Any]
        else {
            log.error("CoreBrightness reported no sun schedule")
            return nil
        }
        let dates = { (keys: [String]) in keys.compactMap { schedule[$0] as? Date } }
        return SunTimes(
            sunrises: dates(["previousSunrise", "sunrise", "nextSunrise"]),
            sunsets: dates(["previousSunset", "sunset", "nextSunset"])
        )
    }

    /// How warm Night Shift tints the display, 0 to 100, or nil when CoreBrightness can't say.
    /// macOS announces no change to it, so it's read whenever it's needed.
    var strength: UInt8? { strengthControl?.value }

    /// Uncommitted, macOS shows the strength without saving it. A committed write reads and
    /// reports the resulting strength, including when CoreBrightness refuses the change.
    func setStrength(_ value: UInt8, commit: Bool) {
        guard let strengthControl else {
            log.error("the strength is unavailable; dropped setStrength(\(value))")
            return
        }
        strengthControl.set(value, commit: commit)
    }

    /// Shows the other side of Night Shift at once, without switching it (ADR-0015): true colour
    /// while it's on, its warmth at the set strength while it's off. `BlueLightReductionFactor`,
    /// the tint macOS fades at each switch, tints the screen even with Night Shift off.
    func peek() {
        guard peekedFrom == nil, let enabled = status?.enabled, let factor = tintFactor,
            setFactor(enabled ? 0 : 1, fade: 0)
        else { return }
        peekedFrom = (enabled, factor)
    }

    /// Ends a peek: the tint goes back to where the peek found it, partway through one of the
    /// schedule's fades too, which macOS then carries on. Where Night Shift switched meanwhile,
    /// the tint goes to what it now calls for.
    func endPeek() {
        guard let from = peekedFrom else { return }
        peekedFrom = nil
        guard let enabled = status?.enabled else { return }
        let factor: Float = enabled == from.enabled ? from.factor : (enabled ? 1 : 0)
        _ = setFactor(factor, fade: 0.2)
    }

    /// The factor outlives Nightcap, so a crash mid-peek leaves Night Shift on with no warmth, or
    /// off with its warmth, which nothing else puts back.
    func repairPeek() {
        guard let enabled = status?.enabled, let factor = tintFactor else { return }
        if enabled, factor == 0 {
            log.notice("Night Shift was on without its tint, as a peek a crash cut short leaves it; putting it back")
            _ = setFactor(1, fade: 0.2)
        } else if !enabled, factor > 0 {
            log.notice("Night Shift was off with its tint, as a peek a crash cut short leaves it; taking it off")
            _ = setFactor(0, fade: 0.2)
        }
    }

    private var tintFactor: Float? {
        let value =
            copyProperty(systemClient, NSSelectorFromString("copyPropertyForKey:"), "BlueLightReductionFactor")?
            .takeRetainedValue() as? [String: Any]
        return (value?["BlueLightReductionFactorValue"] as? NSNumber)?.floatValue
    }

    private func setFactor(_ value: Float, fade: Float) -> Bool {
        guard let setProperty else { return false }
        let factor: NSDictionary = [
            "BlueLightReductionFactorValue": value, "BlueLightReductionFactorFadePeriod": fade, "ForceUpdate": true,
        ]
        guard setProperty(systemClient, NSSelectorFromString("setProperty:forKey:"), factor, "BlueLightReductionFactor")
        else {
            log.error("CoreBrightness refused the tint factor \(value)")
            return false
        }
        return true
    }

    /// macOS applies a new schedule at once, so it can switch Night Shift on or off.
    func setSchedule(_ schedule: Schedule) {
        guard let setMode, let setScheduleImplementation else {
            log.error("the schedule can't be set; dropped setSchedule")
            return
        }
        let mode: Int32
        switch schedule {
        case .off: mode = 0
        case .sunsetToSunrise: mode = 1
        case .custom(let from, let to):
            mode = 2
            // Mode 2 uses whatever custom hours are stored, so they go in first.
            let hours = [Int32(from.hour), Int32(from.minute), Int32(to.hour), Int32(to.minute)]
            let stored = hours.withUnsafeBytes {
                setScheduleImplementation(client, NSSelectorFromString("setSchedule:"), $0.baseAddress!)
            }
            guard stored else {
                log.error("CoreBrightness refused the custom hours")
                return
            }
        }
        guard setMode(client, NSSelectorFromString("setMode:"), mode) else {
            log.error("CoreBrightness refused setMode(\(mode))")
            return
        }
    }

    /// Ends any peek: switching Night Shift on puts the tint back by itself, and switching it off
    /// needs the factor left at 0. Writes nothing while Night Shift is already so, or the same
    /// write is on its way: the core asks again with each input until a report shows it.
    func setEnabled(_ enabled: Bool) {
        guard writes.needs(enabled, current: status?.enabled) else { return }
        guard setEnabledImplementation(client, NSSelectorFromString("setEnabled:"), enabled) else {
            log.error("CoreBrightness refused setEnabled(\(enabled))")
            return
        }
        writes.wrote(enabled)
        peekedFrom = nil
    }

    private static func timeOfDay(_ hour: Int32, _ minute: Int32) -> TimeOfDay? {
        guard (0..<24).contains(hour), (0..<60).contains(minute) else { return nil }
        return TimeOfDay(hour: UInt8(hour), minute: UInt8(minute))
    }
}
