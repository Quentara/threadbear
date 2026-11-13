import AppKit
import SwiftUI

/// The Swift app telling people about the GPUI app (`desktop/`), which is
/// replacing it, with one button that starts using it (Mike's pick, 13 Nov
/// 2026: mockups A and C in the GPUI port thread). Two pieces:
///
/// - A window over the main one, once, when the app starts (`NewAppWindow`):
///   what the new app is, "Not now" and "Start using the new app".
/// - A card at the top of the sidebar (`NewAppCard`) whose "Start using it"
///   brings that window up again; "Later" hides the card for a week.
///
/// Starting runs `desktop/scripts/switch-from-swift.sh` from the checkout's
/// main: Rust if it's missing, the build, the move into ~/Applications,
/// threadbear:// links, then it opens the new app, which brings over this
/// one's settings and drafts itself. This app stays and keeps working.
///
/// Offered only while the GPUI app isn't installed, from an app that knows
/// its checkout (build.json) and whose main has the script.
@MainActor
@Observable
final class NewAppOffer {
    static let shared = NewAppOffer()

    enum Stage: Equatable {
        case idle
        /// What it's doing ("Building the new app") and the latest line under it.
        case working(step: String, detail: String)
        case failed(String)
        case done
    }

    var stage: Stage = .idle
    /// The window over the main one.
    var windowShown = false
    /// Whether this Mac can be offered it at all.
    private(set) var offered = false
    private var hiddenUntil: Date?
    private var checkout: String?
    /// The script's latest line that wasn't a step, for a failure's message.
    private var lastLine = ""

    static let seenKey = "newAppPromptSeen"
    static let laterKey = "newAppCardHiddenUntil"
    static let target = URL(fileURLWithPath: NSHomeDirectory() + "/Applications/Threadbear GPUI.app")
    static let logPath = NSHomeDirectory() + "/Library/Logs/threadbear-new-app.log"
    static let script = "desktop/scripts/switch-from-swift.sh"
    /// Which features the new app has yet, from its behaviour specs.
    static let parityURL = URL(string: "https://github.com/mikecann/threadbear/blob/main/docs/gpui-port/PARITY.md")!

    /// The card: offered, not put off with Later, or under way.
    var cardShown: Bool {
        guard offered else { return false }
        if stage != .idle { return true }
        return (hiddenUntil ?? .distantPast) < Date()
    }

    var working: Bool {
        if case .working = stage { return true }
        return false
    }

    /// At launch, once the threads are in. A scripted run never shows the
    /// window by itself (the automation's `newApp` does).
    func setUp(scripted: Bool) {
        hiddenUntil = UserDefaults.standard.object(forKey: Self.laterKey) as? Date
        guard !FileManager.default.fileExists(atPath: Self.target.path),
              let info = BuildInfo.read(bundle: Bundle.main.bundleURL),
              FileManager.default.fileExists(atPath: info.checkout + "/.git") else { return }
        let checkout = info.checkout
        Task {
            // main has the script: a fork that hasn't brought it in yet
            // would only get an error.
            let result = await AppUpdater.run("/usr/bin/git", ["-C", checkout, "cat-file", "-e", "refs/heads/main:\(Self.script)"])
            guard result.status == 0 else { return }
            self.checkout = checkout
            self.offered = true
            // Once ever, and not over the tour, which has the window first.
            if !scripted, !UserDefaults.standard.bool(forKey: Self.seenKey), Tour.shared.step == nil {
                UserDefaults.standard.set(true, forKey: Self.seenKey)
                windowShown = true
            }
        }
    }

    func notNow() {
        windowShown = false
    }

    func later() {
        let until = Date().addingTimeInterval(7 * 24 * 3600)
        hiddenUntil = until
        UserDefaults.standard.set(until, forKey: Self.laterKey)
    }

    func start() {
        guard let checkout, !working else { return }
        windowShown = true
        stage = .working(step: "Getting ready", detail: "")
        let log = Self.openLog()
        log.write("\n== \(Date()) starting from \(checkout)\n".data(using: .utf8)!)
        let process = Process()
        // The script as main has it, never the working tree, like the app's
        // own updates: whatever an agent has in the checkout stays out.
        process.executableURL = URL(fileURLWithPath: "/usr/sbin/taskpolicy")
        process.arguments = ["-b", "/bin/bash", "-c", """
            set -e
            f="$(mktemp)"; trap 'rm -f "$f"' EXIT
            git -C "$1" show "refs/heads/main:\(Self.script)" > "$f"
            /bin/bash "$f" "$1"
            """, "switch-from-swift", checkout]
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe
        lastLine = ""
        let splitter = LineSplitter()
        // The handler runs on one queue at a time, so the splitter's
        // buffer is only ever touched from there.
        pipe.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            guard !data.isEmpty else { return }
            log.write(data)
            let lines = splitter.feed(data)
            guard !lines.isEmpty else { return }
            Task { @MainActor in
                for line in lines { self.read(line) }
            }
        }
        process.terminationHandler = { finished in
            pipe.fileHandleForReading.readabilityHandler = nil
            let status = finished.terminationStatus
            Task { @MainActor in
                log.write("== exit \(status)\n".data(using: .utf8)!)
                try? log.close()
                self.finished(status: status, lastLine: self.lastLine)
            }
        }
        do {
            try process.run()
        } catch {
            stage = .failed("It couldn't start: \(error.localizedDescription)")
        }
    }

    private func read(_ raw: String) {
        let line = raw.trimmingCharacters(in: .whitespaces)
        guard !line.isEmpty, case .working(let step, _) = stage else { return }
        if line.hasPrefix("STEP ") {
            stage = .working(step: String(line.dropFirst(5)), detail: "")
            return
        }
        lastLine = line
        stage = .working(step: step, detail: Self.detail(line))
    }

    /// Cargo's "Compiling gpui v0.2.0 (…)" as "Compiling gpui"; anything else
    /// as it is.
    static func detail(_ line: String) -> String {
        let words = line.split(separator: " ")
        if words.count >= 2, ["Compiling", "Downloaded", "Downloading"].contains(words[0]) { return "\(words[0]) \(words[1])" }
        return line
    }

    private func finished(status: Int32, lastLine: String) {
        switch status {
        case 0:
            stage = .done
            offered = false
        case 3:
            stage = .failed("Another build of the new app is running. Try again in a few minutes.")
        case 5:
            stage = .failed("This Mac keeps big builds off itself (runner.json has check hosts). Name a Mac you trust to build it in \"updateBuildHosts\" there, then try again.")
        default:
            stage = .failed(lastLine.isEmpty ? "It stopped before the new app was ready." : "It stopped: \(lastLine)")
        }
    }

    func openLog() {
        NSWorkspace.shared.open(URL(fileURLWithPath: Self.logPath))
    }

    /// For the automation's `newApp`: shows the pieces without a checkout or
    /// a build behind them.
    func preview(_ c: [String: Any]) {
        offered = true
        hiddenUntil = nil
        switch c["stage"] as? String {
        case "working": stage = .working(step: "Building the new app", detail: "Compiling gpui")
        case "failed": stage = .failed("It stopped: error: could not compile `gpui`")
        case "done": stage = .done
        default: stage = .idle
        }
        if let shown = c["window"] as? Bool { windowShown = shown }
    }

    private static func openLog() -> FileHandle {
        let url = URL(fileURLWithPath: logPath)
        try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        if !FileManager.default.fileExists(atPath: logPath) { FileManager.default.createFile(atPath: logPath, contents: nil) }
        let handle = (try? FileHandle(forWritingTo: url)) ?? FileHandle.nullDevice
        _ = try? handle.seekToEnd()
        return handle
    }
}

/// Bytes from a pipe in, whole lines out.
private final class LineSplitter: @unchecked Sendable {
    private var pending = Data()

    func feed(_ data: Data) -> [String] {
        pending.append(data)
        var lines: [String] = []
        while let newline = pending.firstIndex(of: 10) {
            lines.append(String(decoding: pending[pending.startIndex..<newline], as: UTF8.self))
            pending = Data(pending[pending.index(after: newline)...])
        }
        return lines
    }
}

private enum NewAppCopy {
    static let windowTitle = "Try the new Threadbear app"
    static let windowBody = "It looks and works exactly like this one, and it runs on Windows and Linux as well as the Mac."
    static let windowCarry = "It opens with your threads, settings and drafts, and this app stays installed so you can come back to it any time."
    static let windowBuild = "The first time, it's built on this Mac, which takes about 10 minutes in the background (and installs Rust first if you need it)."
    static let windowGaps = "A few things still only work here."
    static let cardTitle = "A new Threadbear app"
    static let cardBody = "It looks and works like this one, and runs on Windows and Linux too."
}

@MainActor private var gpuiIcon: some View {
    Group {
        if let image = Art.gpuiApp {
            Image(nsImage: image).resizable().interpolation(.high)
        } else {
            RoundedRectangle(cornerRadius: 8).fill(Color.green.opacity(0.4))
        }
    }
}

/// Mockup A: over the main window, in the middle.
struct NewAppWindow: View {
    @Bindable var offer = NewAppOffer.shared

    var body: some View {
        if offer.windowShown {
            ZStack {
                // Clicks behind it go nowhere while it's up.
                Color.black.opacity(0.18).contentShape(Rectangle())
                    .onTapGesture {}
                card
                    .frame(width: zoomed(400))
                    .padding(zoomed(26))
                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: zoomed(14), style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: zoomed(14), style: .continuous).strokeBorder(Color(nsColor: Theme.cardBorder), lineWidth: 0.5))
                    .shadow(color: .black.opacity(0.3), radius: zoomed(24), y: zoomed(8))
            }
            .transition(.opacity)
        }
    }

    @ViewBuilder private var card: some View {
        VStack(spacing: zoomed(12)) {
            gpuiIcon
                .frame(width: zoomed(72), height: zoomed(72))
                .clipShape(RoundedRectangle(cornerRadius: zoomed(14), style: .continuous))
            Text(NewAppCopy.windowTitle)
                .font(.system(size: TypeScale.scaled(17), weight: .semibold))
            switch offer.stage {
            case .idle: idle
            case .working(let step, let detail): working(step: step, detail: detail)
            case .failed(let message): failed(message)
            case .done: done
            }
        }
        .multilineTextAlignment(.center)
    }

    private var idle: some View {
        VStack(spacing: zoomed(10)) {
            Text(NewAppCopy.windowBody)
            Text(NewAppCopy.windowCarry)
            Text(NewAppCopy.windowBuild)
                .foregroundStyle(.secondary)
            HStack(spacing: zoomed(4)) {
                Text(NewAppCopy.windowGaps).foregroundStyle(.secondary)
                Link("See which", destination: NewAppOffer.parityURL)
                    .tip("What the new app does so far, feature by feature")
            }
            HStack(spacing: zoomed(10)) {
                Button("Not now") { offer.notNow() }
                    .keyboardShortcut(.cancelAction)
                    .buttonStyle(NewAppPrimary(quiet: true))
                    .tip("Close this. The card at the top of the sidebar can bring it back")
                Button("Start using the new app") { offer.start() }
                    .keyboardShortcut(.defaultAction)
                    .buttonStyle(NewAppPrimary())
                    .controlSize(.large)
                    .tip("Install the new app beside this one, bring over your settings and drafts, and open it")
            }
            .padding(.top, zoomed(6))
        }
        .font(.system(size: TypeScale.label))
        .fixedSize(horizontal: false, vertical: true)
    }

    private func working(step: String, detail: String) -> some View {
        VStack(spacing: zoomed(10)) {
            HStack(spacing: zoomed(8)) {
                SpinnerMark().frame(width: zoomed(14), height: zoomed(14))
                Text(step).font(.system(size: TypeScale.label, weight: .medium))
            }
            Text(detail.isEmpty ? " " : detail)
                .font(.system(size: TypeScale.detail, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(1).truncationMode(.middle)
            Text("You can keep using this app while it builds.")
                .font(.system(size: TypeScale.label))
                .foregroundStyle(.secondary)
            Button("Hide") { offer.notNow() }
                .controlSize(.large)
                .tip("Keep building in the background. The card in the sidebar shows how it's going")
                .padding(.top, zoomed(6))
        }
    }

    private func failed(_ message: String) -> some View {
        VStack(spacing: zoomed(10)) {
            Text(message)
                .font(.system(size: TypeScale.label))
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: zoomed(10)) {
                Button("Close") { offer.notNow() }
                    .controlSize(.large)
                    .tip("Close this")
                Button("Show the log") { offer.openLog() }
                    .controlSize(.large)
                    .tip("Open what the install printed, in Console")
                Button("Try again") { offer.start() }
                    .buttonStyle(NewAppPrimary())
                    .controlSize(.large)
                    .tip("Run the install again")
            }
            .padding(.top, zoomed(6))
        }
    }

    private var done: some View {
        VStack(spacing: zoomed(10)) {
            Text("The new app is open. This one keeps working too, so you can come back any time.")
                .font(.system(size: TypeScale.label))
                .fixedSize(horizontal: false, vertical: true)
            Button("Close") { offer.notNow() }
                .keyboardShortcut(.defaultAction)
                .controlSize(.large)
                .tip("Close this")
                .padding(.top, zoomed(6))
        }
    }
}

/// Mockup C: at the top of the sidebar, above Starred.
struct NewAppCard: View {
    @Bindable var offer = NewAppOffer.shared

    var body: some View {
        if offer.cardShown {
            VStack(alignment: .leading, spacing: zoomed(8)) {
                HStack(spacing: zoomed(10)) {
                    gpuiIcon
                        .frame(width: zoomed(30), height: zoomed(30))
                        .clipShape(RoundedRectangle(cornerRadius: zoomed(7), style: .continuous))
                    Text(NewAppCopy.cardTitle)
                        .font(.system(size: TypeScale.sidebarTitle, weight: .semibold))
                }
                switch offer.stage {
                case .working(let step, _):
                    HStack(spacing: zoomed(6)) {
                        SpinnerMark().frame(width: zoomed(12), height: zoomed(12))
                        Text(step).font(.system(size: TypeScale.sidebarPreview)).foregroundStyle(.secondary)
                    }
                    Button("Show") { offer.windowShown = true }
                        .tip("Show how the install is going")
                case .failed:
                    Text("The install stopped.").font(.system(size: TypeScale.sidebarPreview)).foregroundStyle(.secondary)
                    Button("Show") { offer.windowShown = true }
                        .tip("See why, and try again")
                default:
                    Text(NewAppCopy.cardBody)
                        .font(.system(size: TypeScale.sidebarPreview))
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                    HStack(spacing: zoomed(8)) {
                        Button { offer.windowShown = true } label: {
                            Text("Start using it").frame(maxWidth: .infinity)
                        }
                        .buttonStyle(NewAppPrimary())
                        .tip("What the new app is, and a button that installs it")
                        Button("Later") { offer.later() }
                            .buttonStyle(.borderless)
                            .tip("Hide this for a week")
                    }
                }
            }
            .padding(zoomed(12))
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color(nsColor: Theme.cardBackground), in: RoundedRectangle(cornerRadius: zoomed(10), style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: zoomed(10), style: .continuous).strokeBorder(Color(nsColor: Theme.cardBorder), lineWidth: 0.5))
            .padding(.horizontal, SidebarMetrics.outerInset)
            .padding(.top, zoomed(8))
            .transition(.opacity)
        }
    }
}

/// The blue button in both mockups. `.borderedProminent` goes grey whenever
/// the window isn't the key one, which a window over the app at launch
/// often isn't.
private struct NewAppPrimary: ButtonStyle {
    /// The grey one beside it ("Not now").
    var quiet = false
    @Environment(\.isEnabled) private var enabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: TypeScale.label, weight: .medium))
            .foregroundStyle(quiet ? Color.primary : Color.white)
            .padding(.horizontal, zoomed(14))
            .padding(.vertical, zoomed(6))
            .background((quiet ? Color.primary.opacity(0.1) : Color(nsColor: .systemBlue)).opacity(configuration.isPressed ? 0.8 : enabled ? 1 : 0.5),
                        in: RoundedRectangle(cornerRadius: zoomed(7), style: .continuous))
    }
}
