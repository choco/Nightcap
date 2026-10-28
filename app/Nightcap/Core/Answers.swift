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

extension PlaceInFront {
    /// An app's or site's own indefinite rule, which its temporary rules sit over.
    var rule: Rule? { if case .rule(let rule) = base { rule } else { nil } }

    /// What its rules sit on says: an app's or site's rule, or for everywhere the stand-in's on,
    /// else the schedule's say once it's known.
    var baseSays: OnOff? {
        switch base {
        case .rule(let rule): rule.says
        case .standIn: .on
        case .schedule(let state, _): state
        case nil: nil
        }
    }

    /// Whether it has a rule until a time, which is its shortest.
    var changedForATime: Bool {
        rules.contains { if case .until = $0.length { true } else { false } }
    }

    /// Whether one of its rules says `state`.
    func hasRule(saying state: OnOff) -> Bool { rules.contains { $0.says == state } }
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
