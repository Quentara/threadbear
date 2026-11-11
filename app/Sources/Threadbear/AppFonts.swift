import AppKit
import SwiftUI

/// The app's typefaces: Geist for the interface and Geist Mono for code,
/// in place of the system's. Every font in the app comes through here
/// (`.app(size:)` in SwiftUI, `NSFont.appFont(ofSize:)` and its mono and
/// digit kin in AppKit), so the faces change in one place. When Geist
/// isn't installed, the system's own faces stand in.
enum AppFonts {
    static let sans = "Geist"
    static let mono = "GeistMono"

    /// The PostScript name of a family's face at a weight, as the static
    /// .ttf files name them: Geist-SemiBold, GeistMono-Regular.
    static func faceName(_ family: String, _ weight: NSFont.Weight) -> String {
        "\(family)-\(styleName(weight))"
    }

    private static func styleName(_ weight: NSFont.Weight) -> String {
        switch weight.rawValue {
        case ..<(-0.7): "Thin"
        case ..<(-0.5): "ExtraLight"
        case ..<(-0.2): "Light"
        case ..<0.1: "Regular"
        case ..<0.27: "Medium"
        case ..<0.35: "SemiBold"
        case ..<0.5: "Bold"
        case ..<0.6: "ExtraBold"
        default: "Black"
        }
    }

    /// Whether the family is on this Mac, looked up once.
    static let installed: Bool = NSFont(name: faceName(sans, .regular), size: 13) != nil
    static let monoInstalled: Bool = NSFont(name: faceName(mono, .regular), size: 13) != nil

    static func nsWeight(_ weight: Font.Weight) -> NSFont.Weight {
        switch weight {
        case .ultraLight: .ultraLight
        case .thin: .thin
        case .light: .light
        case .medium: .medium
        case .semibold: .semibold
        case .bold: .bold
        case .heavy: .heavy
        case .black: .black
        default: .regular
        }
    }
}

extension Font {
    /// In place of `.system(size:weight:design:)`: Geist, or Geist Mono for
    /// `.monospaced`. Rounded and serif stay the system's.
    static func app(size: CGFloat, weight: Font.Weight = .regular, design: Font.Design = .default) -> Font {
        switch design {
        case .monospaced where AppFonts.monoInstalled:
            return .custom(AppFonts.faceName(AppFonts.mono, AppFonts.nsWeight(weight)), fixedSize: size)
        case .default where AppFonts.installed:
            return .custom(AppFonts.faceName(AppFonts.sans, AppFonts.nsWeight(weight)), fixedSize: size)
        default:
            return .system(size: size, weight: weight, design: design)
        }
    }

    /// In place of `.system(.body, design: .monospaced)` and the like.
    static func app(_ style: Font.TextStyle, design: Font.Design = .default) -> Font {
        let (size, weight): (CGFloat, Font.Weight) = switch style {
        case .largeTitle: (26, .regular)
        case .title: (22, .regular)
        case .title2: (17, .regular)
        case .title3: (15, .regular)
        case .headline: (13, .bold)
        case .subheadline: (11, .regular)
        case .callout: (12, .regular)
        case .footnote: (10, .regular)
        case .caption: (10, .regular)
        case .caption2: (10, .regular)
        default: (13, .regular)
        }
        return .app(size: size, weight: weight, design: design)
    }
}

extension NSFont {
    /// In place of `systemFont(ofSize:weight:)`.
    static func appFont(ofSize size: CGFloat, weight: NSFont.Weight = .regular) -> NSFont {
        guard AppFonts.installed, let font = NSFont(name: AppFonts.faceName(AppFonts.sans, weight), size: size) else {
            return .systemFont(ofSize: size, weight: weight)
        }
        return font
    }

    /// In place of `monospacedSystemFont(ofSize:weight:)`: for code.
    static func appMonoFont(ofSize size: CGFloat, weight: NSFont.Weight = .regular) -> NSFont {
        guard AppFonts.monoInstalled, let font = NSFont(name: AppFonts.faceName(AppFonts.mono, weight), size: size) else {
            return .monospacedSystemFont(ofSize: size, weight: weight)
        }
        return font
    }

    /// In place of `monospacedDigitSystemFont(ofSize:weight:)`: Geist with
    /// its tabular figures, so counts and times don't jiggle.
    static func appDigitFont(ofSize size: CGFloat, weight: NSFont.Weight = .regular) -> NSFont {
        let base = appFont(ofSize: size, weight: weight)
        guard AppFonts.installed else { return .monospacedDigitSystemFont(ofSize: size, weight: weight) }
        let tabular = base.fontDescriptor.addingAttributes([
            .featureSettings: [[
                NSFontDescriptor.FeatureKey.typeIdentifier: kNumberSpacingType,
                NSFontDescriptor.FeatureKey.selectorIdentifier: kMonospacedNumbersSelector,
            ]],
        ])
        return NSFont(descriptor: tabular, size: size) ?? base
    }
}
