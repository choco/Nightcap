import Foundation
import NightcapCore

/// What the notification for a refused command says: why the core refused it, from what the
/// refusal carries.
extension Wording {
    /// A refused command's notification's title.
    var refused: String { text("Nightcap couldn’t make that change") }

    /// Why the core refused a command, in a sentence, with `rule` naming the app or site a
    /// refusal is about: "Your config’s rule for Figma keeps Night Shift off. Change it there."
    func refusal(_ refusal: Refusal, rule: (RuleIdentity) -> Place) -> String {
        func name(_ identity: RuleIdentity) -> String {
            switch rule(identity) {
            case .app(let name): name
            case .site(let domain): domain
            }
        }
        return switch refusal {
        case .nightShiftUnavailable: text("Night Shift isn’t available on this Mac.")
        case .strengthUnavailable: text("Night Shift’s strength can’t be read on this Mac.")
        case .strengthOutOfRange(let strength): text("A strength is 0 to 100, not \(Int(strength)).")
        case .notATimeOfDay: text("Custom hours are times of day.")
        case .tooShort: text("A length is longer than zero.")
        case .tooLong: text("That length is too long.")
        case .passed: text("That time has passed.")
        case .noSchedule: text("There’s no schedule to wait for.")
        case .indefiniteIsAdded: text("Only everywhere is set for good: an app or a site gets a rule instead.")
        case .configSays(let identity, .on):
            text("Your config’s rule for \(name(identity)) keeps Night Shift on.") + " " + text("Change it there.")
        case .configSays(let identity, .off):
            text("Your config’s rule for \(name(identity)) keeps Night Shift off.") + " " + text("Change it there.")
        case .configRule(let identity):
            text("The rule for \(name(identity)) is your config’s.") + " " + text("Change it there.")
        case .noSuchRule(let identity): text("There’s no rule for \(name(identity)).")
        case .adHocRulesOff(let error): text("Ad-hoc rules are off: \(located(error)).")
        case .adHocRulesNotSaved(let path, let failure): notSaved(path, failure)
        case .invalidConfig(let error): text("Your config is invalid: \(located(error)).")
        case .websiteCheckingInConfig: text("Your config sets website checking, with `websites`.")
        case .noUpdater(.devBuild): text("Dev builds don’t update themselves: rebuild instead.")
        case .noUpdater(.managedInstall): text("This install is kept up to date by its installer.")
        case .noLoginItem(.devBuild): text("Dev builds don’t open themselves at login.")
        case .noLoginItem(.managedInstall): text("This install doesn’t open itself at login: its installer starts it.")
        case .nothingToAccept(.nightShift):
            text("Night Shift’s schedule is as you set it up, so there’s nothing to accept.")
        case .nothingToAccept(.accessibility): text("Accessibility is as you set it up, so there’s nothing to accept.")
        case .nothingToAccept(.notifications):
            text("Notifications are as you set them up, so there’s nothing to accept.")
        case .nothingToAccept(.openAtLogin):
            text("Opening at login is as you set it up, so there’s nothing to accept.")
        case .nothingToAccept(.configFile): text("Problems with your config can only be fixed, not accepted.")
        case .nothingToAccept(.adHocRules): text("Problems with ad-hoc rules can only be fixed, not accepted.")
        }
    }

    /// Why macOS couldn't save `rules.toml` at `path`, in a sentence: "Couldn’t save /…/rules.toml:
    /// a folder is in its place."
    private func notSaved(_ path: String, _ failure: IoFailure) -> String {
        switch failure {
        case .notFound: text("Couldn’t save \(path): a folder on its path isn’t there.")
        case .permissionDenied: text("Nightcap isn’t allowed to save \(path).")
        case .isAFolder: text("Couldn’t save \(path): a folder is in its place.")
        case .notAFolder: text("Couldn’t save \(path): one of the folders on its path is a file.")
        case .notUtf8: text("Couldn’t save \(path): it isn’t UTF-8 text.")
        case .other: text("Couldn’t save \(path).")
        }
    }
}
