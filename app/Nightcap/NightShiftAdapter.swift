import Foundation
import NightcapCore
import ObjectiveC
import os

/// Drives Night Shift through the private CoreBrightness framework. Everything is looked up at
/// runtime and checked against the method signatures seen on macOS 27, so an OS update that
/// changes them leaves Night Shift unavailable instead of crashing (ADR-0005).
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
