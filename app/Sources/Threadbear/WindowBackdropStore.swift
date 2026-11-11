import AppKit
import CoreImage
import Observation
import ThreadbearCore
import UniformTypeIdentifiers

/// The picture behind the main window: the painting that ships with the
/// app, or one picked in Settings > General > Appearance. A picked one is
/// copied into the data folder (`window-backdrop.<ext>`), so it stays when
/// the original moves, and its name kept in UserDefaults. Frosted once as
/// it loads, at most 2400 pixels wide, so drawing it costs no more than a
/// plain colour.
@MainActor @Observable
final class WindowBackdropStore {
    static let shared = WindowBackdropStore()

    static let fileKey = "windowBackdropFile"
    /// How soft the picture goes behind the panels, as frosted glass, in
    /// pixels of a picture `maxWidth` wide; narrower ones get less.
    static let frostRadius: Double = 10
    static let maxWidth: CGFloat = 2400

    /// What's drawn, frosted. nil only when no picture could be read at all.
    private(set) var image: NSImage?
    /// The unblurred picture, for the thumbnail in Settings.
    private(set) var preview: NSImage?
    /// Whether it's a picked picture rather than the painting.
    private(set) var isCustom = false

    private let dataDir: URL
    private let defaults: UserDefaults

    init(dataDir: URL = RunnerConfig.dataDir(), defaults: UserDefaults = .standard) {
        self.dataDir = dataDir
        self.defaults = defaults
        load()
    }

    var customURL: URL? {
        defaults.string(forKey: Self.fileKey).map { dataDir.appendingPathComponent($0) }
    }

    /// Copies the picture in and shows it. Throws when it can't be read
    /// as a picture or copied.
    func use(_ source: URL) throws {
        guard NSImage(contentsOf: source) != nil else { throw BackdropError.notAPicture }
        let ext = source.pathExtension.isEmpty ? "png" : source.pathExtension.lowercased()
        let name = "window-backdrop.\(ext)"
        let target = dataDir.appendingPathComponent(name)
        let fm = FileManager.default
        try fm.createDirectory(at: dataDir, withIntermediateDirectories: true)
        // The one before, whatever its extension.
        if let old = customURL { try? fm.removeItem(at: old) }
        try? fm.removeItem(at: target)
        try fm.copyItem(at: source, to: target)
        defaults.set(name, forKey: Self.fileKey)
        load()
    }

    /// Back to the painting; the picked copy is deleted.
    func useDefault() {
        if let old = customURL { try? FileManager.default.removeItem(at: old) }
        defaults.removeObject(forKey: Self.fileKey)
        load()
    }

    private func load() {
        let custom = customURL.flatMap { NSImage(contentsOf: $0) }
        isCustom = custom != nil
        let sharp = (custom ?? Art.windowPainting).map { Self.scaled($0, maxWidth: Self.maxWidth) }
        preview = sharp
        // The same softness at any size: a small picture is stretched
        // over the window, so it gets a smaller blur before it is.
        image = sharp.map { pic in
            let width = pic.cgImage(forProposedRect: nil, context: nil, hints: nil).map { CGFloat($0.width) } ?? Self.maxWidth
            return Self.frosted(pic, radius: max(1.5, Self.frostRadius * Double(width / Self.maxWidth)))
        }
    }

    /// At most `maxWidth` pixels wide, so a camera's 8000-pixel photo
    /// isn't blurred and drawn at full size.
    static func scaled(_ picture: NSImage, maxWidth: CGFloat) -> NSImage {
        guard let cg = picture.cgImage(forProposedRect: nil, context: nil, hints: nil),
              CGFloat(cg.width) > maxWidth else { return picture }
        let scale = maxWidth / CGFloat(cg.width)
        let input = CIImage(cgImage: cg).transformed(by: CGAffineTransform(scaleX: scale, y: scale))
        guard let out = CIContext().createCGImage(input, from: input.extent) else { return picture }
        return NSImage(cgImage: out, size: NSSize(width: out.width, height: out.height))
    }

    /// Blurred once, edges clamped first so they don't fade to clear.
    static func frosted(_ picture: NSImage, radius: Double) -> NSImage {
        guard radius > 0,
              let cg = picture.cgImage(forProposedRect: nil, context: nil, hints: nil) else { return picture }
        let input = CIImage(cgImage: cg)
        let blurred = input.clampedToExtent()
            .applyingGaussianBlur(sigma: radius)
            .cropped(to: input.extent)
        guard let out = CIContext().createCGImage(blurred, from: input.extent) else { return picture }
        return NSImage(cgImage: out, size: picture.size)
    }

    /// Asks for a picture with the open panel and uses it. Says why in an
    /// alert when it can't.
    func choose() {
        let panel = NSOpenPanel()
        panel.title = "Choose a background picture"
        panel.prompt = "Use as background"
        panel.allowedContentTypes = [.image]
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        do {
            try use(url)
        } catch {
            let alert = NSAlert()
            alert.messageText = "Couldn't use that picture"
            alert.informativeText = (error as? BackdropError)?.message ?? error.localizedDescription
            alert.runModal()
        }
    }

    enum BackdropError: Error {
        case notAPicture
        var message: String { "It isn't a picture Threadbear can read. Try a JPEG, PNG or HEIC." }
    }
}
