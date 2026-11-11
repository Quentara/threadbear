import AppKit
import XCTest
@testable import Threadbear

/// The background picture: a picked one is copied in, frosted and shown,
/// and going back to the painting deletes the copy.
@MainActor
final class WindowBackdropStoreTests: XCTestCase {
    private var dir: URL!
    private var defaults: UserDefaults!
    private let suite = "WindowBackdropStoreTests"

    override func setUp() {
        super.setUp()
        dir = FileManager.default.temporaryDirectory.appendingPathComponent("backdrop-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defaults = UserDefaults(suiteName: suite)
        defaults.removePersistentDomain(forName: suite)
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: dir)
        defaults.removePersistentDomain(forName: suite)
        super.tearDown()
    }

    /// A red PNG, `width` pixels wide, outside the data folder.
    private func picture(width: Int, height: Int = 20) throws -> URL {
        let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height, bitsPerSample: 8,
                                   samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                                   bytesPerRow: 0, bitsPerPixel: 0)!
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
        NSColor.red.setFill()
        NSRect(x: 0, y: 0, width: width, height: height).fill()
        NSGraphicsContext.restoreGraphicsState()
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("pick-\(UUID().uuidString).png")
        try rep.representation(using: .png, properties: [:])!.write(to: url)
        return url
    }

    func testAPickedPictureIsCopiedInAndShownThenTheDefaultDeletesIt() throws {
        let store = WindowBackdropStore(dataDir: dir, defaults: defaults)
        XCTAssertFalse(store.isCustom)

        let source = try picture(width: 40)
        defer { try? FileManager.default.removeItem(at: source) }
        try store.use(source)
        XCTAssertTrue(store.isCustom)
        let copy = try XCTUnwrap(store.customURL)
        XCTAssertEqual(copy.lastPathComponent, "window-backdrop.png")
        XCTAssertTrue(FileManager.default.fileExists(atPath: copy.path))
        XCTAssertNotNil(store.image)
        XCTAssertEqual(store.preview?.size.width, 40)

        // Still there for the next launch.
        XCTAssertTrue(WindowBackdropStore(dataDir: dir, defaults: defaults).isCustom)

        store.useDefault()
        XCTAssertFalse(store.isCustom)
        XCTAssertNil(store.customURL)
        XCTAssertFalse(FileManager.default.fileExists(atPath: copy.path))
    }

    func testSomethingThatIsntAPictureIsRefusedAndTheOldOneStays() throws {
        let store = WindowBackdropStore(dataDir: dir, defaults: defaults)
        let first = try picture(width: 30)
        defer { try? FileManager.default.removeItem(at: first) }
        try store.use(first)
        let text = dir.appendingPathComponent("notes.txt")
        try Data("hello".utf8).write(to: text)
        XCTAssertThrowsError(try store.use(text))
        XCTAssertTrue(store.isCustom)
        XCTAssertEqual(store.preview?.size.width, 30)
    }

    func testABigPhotoIsScaledDownBeforeItsBlurred() throws {
        let store = WindowBackdropStore(dataDir: dir, defaults: defaults)
        let big = try picture(width: 3000)
        defer { try? FileManager.default.removeItem(at: big) }
        try store.use(big)
        XCTAssertEqual(store.preview?.size.width, WindowBackdropStore.maxWidth)
    }
}
