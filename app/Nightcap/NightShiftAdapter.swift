import Foundation
import NightcapCore
import ObjectiveC
import os

/// Drives Night Shift through the private CoreBrightness framework. Everything is looked up at
/// runtime and checked against the method signatures seen on macOS 15 and 27, so an OS update
/// that changes them leaves Night Shift unavailable instead of crashing (ADR-0005).
final class NightShiftAdapter {
    struct Status {
        var enabled: Bool
        var schedule: Schedule
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
            "copyPropertyForKey:": "@24@0:8@16",
        ],
    ]
    /// Checked apart from the rest: without them Night Shift still works, with no strength to
    /// show or set (ADR-0020).
    private static let strengthEncodings = [
        "getStrength:": "B24@0:8^f16",
        "setStrength:commit:": "B24@0:8f16B20",
    ]
    /// Checked apart from the rest too: without them, onboarding sends the user to System
    /// Settings to choose the schedule (ADR-0020). The custom hours are
    /// {{int hour, int minute} from, {int hour, int minute} to}.
    private static let scheduleEncodings = [
        "setMode:": "B20@0:8i16",
        "setSchedule:": "B24@0:8r^{?={?=ii}{?=ii}}16",
    ]
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
    private let getStrength: GetStrength?
    private let setStrengthImplementation: SetStrength?
    private let setMode: SetMode?
    private let setScheduleImplementation: SetSchedule?

    /// Fails when CoreBrightness or its expected methods are missing. `onChange` runs on the main
    /// thread with the new status whenever CoreBrightness reports a status change.
    init?(onChange: @escaping @MainActor (Status) -> Void) {
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
                    log.fault("\(className, privacy: .public).\(name, privacy: .public) is \(encoding ?? "missing", privacy: .public), expected \(expected, privacy: .public)")
                    return nil
                }
            }
            classes[className] = found
        }
        client = classes["CBBlueLightClient"]!.init()
        systemClient = classes["BrightnessSystemClient"]!.init()
        getStatus = unsafeBitCast(client.method(for: NSSelectorFromString("getBlueLightStatus:")), to: GetStatus.self)
        setEnabledImplementation = unsafeBitCast(client.method(for: NSSelectorFromString("setEnabled:")), to: SetEnabled.self)
        copyProperty = unsafeBitCast(systemClient.method(for: NSSelectorFromString("copyPropertyForKey:")), to: CopyProperty.self)
        let found = { (encodings: [String: String], what: String) in
            encodings.allSatisfy { name, expected in
                let method = class_getInstanceMethod(classes["CBBlueLightClient"]!, NSSelectorFromString(name))
                let encoding = method.flatMap { method_getTypeEncoding($0) }.map { String(cString: $0) }
                if encoding != expected {
                    log.fault("CBBlueLightClient.\(name, privacy: .public) is \(encoding ?? "missing", privacy: .public), expected \(expected, privacy: .public); \(what, privacy: .public) can't be set")
                }
                return encoding == expected
            }
        }
        let strengthFound = found(Self.strengthEncodings, "the strength")
        let scheduleFound = found(Self.scheduleEncodings, "the schedule")
        getStrength = strengthFound
            ? unsafeBitCast(client.method(for: NSSelectorFromString("getStrength:")), to: GetStrength.self) : nil
        setStrengthImplementation = strengthFound
            ? unsafeBitCast(client.method(for: NSSelectorFromString("setStrength:commit:")), to: SetStrength.self) : nil
        setMode = scheduleFound
            ? unsafeBitCast(client.method(for: NSSelectorFromString("setMode:")), to: SetMode.self) : nil
        setScheduleImplementation = scheduleFound
            ? unsafeBitCast(client.method(for: NSSelectorFromString("setSchedule:")), to: SetSchedule.self) : nil

        let setBlock = unsafeBitCast(client.method(for: NSSelectorFromString("setStatusNotificationBlock:")), to: SetBlock.self)
        // CoreBrightness calls this on one of its own queues, often several times per change.
        setBlock(client, NSSelectorFromString("setStatusNotificationBlock:")) { [weak self] in
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    guard let status = self?.status else { return }
                    onChange(status)
                }
            }
        }
    }

    /// Night Shift's state and schedule, or nil when CoreBrightness can't say.
    var status: Status? {
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
        return Status(enabled: raw.load(fromByteOffset: Self.enabledOffset, as: Bool.self), schedule: schedule)
    }

    /// The sunrises and sunsets CoreBrightness schedules by, from the day before to the day
    /// after, or nil when it reports none.
    var sunTimes: SunTimes? {
        let selector = NSSelectorFromString("copyPropertyForKey:")
        guard let schedule = copyProperty(systemClient, selector, "BlueLightSunSchedule")?
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
    var strength: UInt8? {
        guard let getStrength else { return nil }
        var value: Float = -1
        guard getStrength(client, NSSelectorFromString("getStrength:"), &value), (0...1).contains(value) else {
            log.error("CoreBrightness didn't report a usable strength")
            return nil
        }
        return UInt8((value * 100).rounded())
    }

    /// Uncommitted, macOS shows the strength, even with Night Shift off, but doesn't save it.
    func setStrength(_ value: UInt8, commit: Bool) {
        guard let setStrengthImplementation else {
            log.error("the strength is unavailable; dropped setStrength(\(value))")
            return
        }
        let selector = NSSelectorFromString("setStrength:commit:")
        guard setStrengthImplementation(client, selector, Float(value) / 100, commit) else {
            log.error("CoreBrightness refused setStrength(\(value), commit: \(commit))")
            return
        }
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

    func setEnabled(_ enabled: Bool) {
        guard setEnabledImplementation(client, NSSelectorFromString("setEnabled:"), enabled) else {
            log.error("CoreBrightness refused setEnabled(\(enabled))")
            return
        }
    }

    private static func timeOfDay(_ hour: Int32, _ minute: Int32) -> TimeOfDay? {
        guard (0..<24).contains(hour), (0..<60).contains(minute) else { return nil }
        return TimeOfDay(hour: UInt8(hour), minute: UInt8(minute))
    }
}
