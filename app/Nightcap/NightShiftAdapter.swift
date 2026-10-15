import Foundation
import ObjectiveC
import os

/// Drives Night Shift through the private CoreBrightness framework. Everything is looked up at
/// runtime and checked against the method signatures seen on macOS 27, so an OS update that
/// changes them leaves Night Shift unavailable instead of crashing (ADR-0005).
final class NightShiftAdapter {
    private typealias GetStatus = @convention(c) (NSObject, Selector, UnsafeMutableRawPointer) -> Bool
    private typealias SetEnabled = @convention(c) (NSObject, Selector, Bool) -> Bool
    private typealias SetBlock = @convention(c) (NSObject, Selector, @escaping @convention(block) () -> Void) -> Void

    private static let frameworkPath = "/System/Library/PrivateFrameworks/CoreBrightness.framework/CoreBrightness"

    /// The status struct is {BOOL active, BOOL enabled, BOOL sunSchedulePermitted, int mode,
    /// {{int, int}, {int, int}} schedule, unsigned long long disableFlags, BOOL available}.
    private static let expectedEncodings = [
        "getBlueLightStatus:": "B24@0:8^{?=BBBi{?={?=ii}{?=ii}}QB}16",
        "setEnabled:": "B20@0:8B16",
        "setStatusNotificationBlock:": "v24@0:8@?16",
    ]
    private static let statusSize = 40
    private static let enabledOffset = 1
    private static let availableOffset = 32

    private let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "nightshift")
    private let client: NSObject
    private let getStatus: GetStatus
    private let setEnabledImplementation: SetEnabled

    /// Fails when CoreBrightness or its expected methods are missing. `onChange` runs on the main
    /// thread with the new enabled state whenever CoreBrightness reports a status change.
    init?(onChange: @escaping @MainActor (Bool) -> Void) {
        let log = Logger(subsystem: Bundle.main.bundleIdentifier!, category: "nightshift")
        guard dlopen(Self.frameworkPath, RTLD_NOW) != nil,
              let clientClass = NSClassFromString("CBBlueLightClient") as? NSObject.Type
        else {
            log.fault("CoreBrightness or CBBlueLightClient is missing")
            return nil
        }
        for (name, expected) in Self.expectedEncodings {
            let method = class_getInstanceMethod(clientClass, NSSelectorFromString(name))
            let encoding = method.flatMap { method_getTypeEncoding($0) }.map { String(cString: $0) }
            guard encoding == expected else {
                log.fault("CBBlueLightClient.\(name, privacy: .public) is \(encoding ?? "missing", privacy: .public), expected \(expected, privacy: .public)")
                return nil
            }
        }
        client = clientClass.init()
        getStatus = unsafeBitCast(client.method(for: NSSelectorFromString("getBlueLightStatus:")), to: GetStatus.self)
        setEnabledImplementation = unsafeBitCast(client.method(for: NSSelectorFromString("setEnabled:")), to: SetEnabled.self)

        let setBlock = unsafeBitCast(client.method(for: NSSelectorFromString("setStatusNotificationBlock:")), to: SetBlock.self)
        // CoreBrightness calls this on one of its own queues, often several times per change.
        setBlock(client, NSSelectorFromString("setStatusNotificationBlock:")) { [weak self] in
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    guard let enabled = self?.enabled else { return }
                    onChange(enabled)
                }
            }
        }
    }

    /// Whether Night Shift is on, or nil when CoreBrightness can't say.
    var enabled: Bool? {
        let status = UnsafeMutableRawPointer.allocate(byteCount: Self.statusSize, alignment: 8)
        defer { status.deallocate() }
        status.initializeMemory(as: UInt8.self, repeating: 0, count: Self.statusSize)
        guard getStatus(client, NSSelectorFromString("getBlueLightStatus:"), status),
              status.load(fromByteOffset: Self.availableOffset, as: Bool.self)
        else {
            log.error("CoreBrightness didn't report a usable status")
            return nil
        }
        return status.load(fromByteOffset: Self.enabledOffset, as: Bool.self)
    }

    func setEnabled(_ enabled: Bool) {
        guard setEnabledImplementation(client, NSSelectorFromString("setEnabled:"), enabled) else {
            log.error("CoreBrightness refused setEnabled(\(enabled))")
            return
        }
    }
}
