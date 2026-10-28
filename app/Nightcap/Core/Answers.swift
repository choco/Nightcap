import Foundation
import NightcapCore

/// The core's answers as the menu-bar UI reads them.
extension OnOff {
    var other: OnOff { self == .on ? .off : .on }
}

extension Length {
    /// When a rule of this length ends, if it has an end.
    var end: Date? {
        switch self {
        case .indefinite: nil
        case .untilScheduleTurns(let until): until
        case .until(let until): until
        }
    }

    /// Where a rule of this length goes among its place's, shortest first.
    var rank: Int {
        switch self {
        case .until: 0
        case .untilScheduleTurns: 1
        case .indefinite: 2
        }
    }
}

extension Place {
    /// The identity an app's or site's indefinite rule here has; everywhere has none.
    var identity: RuleIdentity? {
        switch self {
        case .everywhere: nil
        case .app(let bundleId): .app(bundleId)
        case .domain(let domain): .domain(domain)
        }
    }
}

extension CheckDetail {
    var kind: CheckKind {
        switch self {
        case .nightShift: .nightShift
        case .accessibility: .accessibility
        case .notifications: .notifications
        case .openAtLogin: .openAtLogin
        case .configFile: .configFile
        case .adHocRules: .adHocRules
        }
    }
}

extension CheckState {
    /// A check that needs the user: drift, or broken.
    var isProblem: Bool { self == .drifted || self == .broken }
}
