import Foundation
import NightcapCore

/// Announces each problem once as it appears on its own (App glossary), from the snapshot each
/// outcome hands over. A problem is a check's while it lasts, by its kind, whatever it finds
/// meanwhile.
///
/// - A fact's outcome, the launch's among them, tells a problem its snapshot shows that wasn't
///   told yet, if `Notice` words it. One it doesn't, such as Night Shift unavailable, which is
///   how the core answers before the first report, is told once it turns into one it does.
/// - A command's outcome, or quitting's, tells nothing: what the user causes is in front of them
///   already, and stays untold while it lasts.
/// - A problem that ends, its check no problem in a snapshot, is forgotten, so one that comes
///   back is told again. A check the snapshot doesn't hold yet, such as a grant macOS hasn't
///   reported since the launch, keeps what was told.
///
/// What was told is kept in `defaults`, so a relaunch doesn't tell a standing problem again. The
/// first run, with nothing kept, tells those standing then once, as any that appear.
struct Announcer {
    let defaults: UserDefaults
    let words: Wording

    private static let told = "toldProblems"

    /// The notices for the problems `snapshot` shows that appeared on their own, as of an outcome
    /// that answers `cause`, and records what it told.
    func heard(_ cause: Cause, _ snapshot: Snapshot) -> [Notice] {
        let checks = snapshot.checks().checks
        let problems = checks.filter { $0.state.isProblem }
        let record = Set(defaults.stringArray(forKey: Self.told) ?? [])
        var told = record.subtracting(checks.filter { !$0.state.isProblem }.map(\.detail.kind.key))
        var notices: [Notice] = []
        for problem in problems where !told.contains(problem.detail.kind.key) {
            switch cause {
            case .launch, .fact:
                guard let notice = Notice(problem: problem.detail, words: words) else { continue }
                notices.append(notice)
            case .command, .quit:
                break
            }
            told.insert(problem.detail.kind.key)
        }
        if told != record { keep(told) }
        return notices
    }

    private func keep(_ told: Set<String>) {
        defaults.set(told.sorted(), forKey: Self.told)
    }
}

extension CheckKind {
    /// How the record of what was told names the check: as the protocol does.
    fileprivate var key: String {
        switch self {
        case .nightShift: "night_shift"
        case .accessibility: "accessibility"
        case .notifications: "notifications"
        case .openAtLogin: "open_at_login"
        case .configFile: "config_file"
        case .adHocRules: "ad_hoc_rules"
        }
    }
}
