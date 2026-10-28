import AppKit

extension NSFont {
    /// The wordmark's face, Caprasimo, which the app bundles beside its licence. Missing, it's a
    /// broken bundle, not something to work around.
    static func nightcapWordmark(size: CGFloat) -> NSFont {
        registerBundledFonts
        guard let font = NSFont(name: "Caprasimo-Regular", size: size) else {
            fatalError("Caprasimo isn't in the app bundle")
        }
        return font
    }

    private static let registerBundledFonts: Void = {
        guard let url = Bundle.main.url(forResource: "Caprasimo-Regular", withExtension: "ttf") else {
            fatalError("Caprasimo-Regular.ttf isn't in the app bundle")
        }
        var error: Unmanaged<CFError>?
        if !CTFontManagerRegisterFontsForURL(url as CFURL, .process, &error) {
            fatalError("couldn't register Caprasimo: \(error!.takeRetainedValue())")
        }
    }()
}
