import AppKit

/// A patch's picture icon (`iconFile`): an image file, or a .app's own icon
/// file (`applicationIcon`). Kept once read, since the rail and every chip ask on
/// each redraw; a file that can't be read is nil and the symbol shows.
@MainActor
enum PatchImage {
    private static var cache: [String: NSImage?] = [:]

    static func image(_ path: String?) -> NSImage? {
        guard let path, !path.isEmpty else { return nil }
        if let hit = cache[path] { return hit }
        let image: NSImage? = path.hasSuffix(".app") ? applicationIcon(path) : NSImage(contentsOfFile: path)
        cache[path] = image
        return image
    }

    /// A .app's icon, read as a file from inside it: the icns its
    /// Info.plist names in Contents/Resources. Never NSWorkspace's icon for
    /// it, which can register the bundle with Launch Services, so a folder's
    /// .app claiming Chrome's id could be what a launch by that id starts
    /// (Daybreak 112). An app whose icon is only in an asset catalog shows
    /// the patch's symbol.
    static func applicationIcon(_ path: String) -> NSImage? {
        let bundle = URL(fileURLWithPath: path).resolvingSymlinksInPath()
        func inside(_ url: URL) -> URL? {
            let real = url.resolvingSymlinksInPath()
            return real.path.hasPrefix(bundle.path + "/") ? real : nil
        }
        guard let plist = inside(bundle.appendingPathComponent("Contents/Info.plist")),
              let size = try? plist.resourceValues(forKeys: [.fileSizeKey]).fileSize, size <= 1024 * 1024,
              let data = try? Data(contentsOf: plist),
              let info = try? PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any],
              let name = info["CFBundleIconFile"] as? String,
              !name.isEmpty, !name.hasPrefix("."), !name.contains("/")
        else { return nil }
        let file = name.lowercased().hasSuffix(".icns") ? name : name + ".icns"
        guard let icon = inside(bundle.appendingPathComponent("Contents/Resources").appendingPathComponent(file)),
              let iconSize = try? icon.resourceValues(forKeys: [.fileSizeKey]).fileSize, iconSize <= 32 * 1024 * 1024
        else { return nil }
        return NSImage(contentsOf: icon)
    }
}
