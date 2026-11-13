// Real mouse events for a drag test on Tinker Desk's desktop (never Mike's, as
// it takes the pointer: app/README.md, "A real drag"):
//   realdrag <fromX> <fromY> <toX> <toY> <holdSeconds> <shotPath>
// Screen points, from the top left. With a path, and a window number in
// RD_WINDOW, takes a picture of that window mid-drag before letting go.
//
// Guarded (features/tools/real-screen.ts's realDrag sets these): RD_GUARD is
// a program printing the pid of the window on top at `<x> <y>` (window-at,
// which knows when the Dock's see-through overlay lets a click through),
// RD_PRESS_PID the app that must be under the press, RD_DROP_PID the one
// that must be under the drop. The caller checked before starting this; the
// hover and the drag take a second or so, so it looks again: at the press
// after the hover (nothing pressed if it's changed hands), before every
// step of the drag (nothing on top there but the two apps, or a window
// that was already over the press and that the guard let through), and at
// the drop before letting go. A step or drop that's changed hands cancels:
// Escape (a drag-and-drop's cancel), then back along the checked steps to
// the press point (short of a row swipe's line, and a moved window back
// where it was), and the button comes up there, at the app, never at
// whatever came up. Exit 3 says so. A
// guard program that fails, hangs past 2 s or says nothing counts as changed.
import AppKit
import CoreGraphics

enum Input: Equatable { case move(CGPoint), down(CGPoint), dragged(CGPoint), up(CGPoint), escape }
struct Pids { var press: Int32; var drop: Int32 }
/// What the drag reads from the screen, so a test can hand it a made-up one.
struct Look {
    /// The guard program's answer at a point: the pid on top, nil if it failed.
    var owner: (CGPoint) -> Int32?
    /// Windows over a point, front to back.
    var stack: (CGPoint) -> [(number: Int, pid: Int32)]
}

func drag(from: CGPoint, to: CGPoint, hold: Double, press: Double, pids: Pids?, look: Look,
          post: (Input) -> Void, wait: (Double) -> Void, shoot: () -> Void, say: (String) -> Void) -> Int32 {
    post(.move(from)); wait(0.25)
    var through: Set<Int> = []
    if let pids {
        guard look.owner(from) == pids.press else { say("the press point isn't the app's any more; nothing pressed"); return 3 }
        // What's over the press point but above the app's window there, which
        // the guard has just let through: the Dock's see-through overlay.
        let over = look.stack(from)
        guard let mine = over.firstIndex(where: { $0.pid == pids.press }) else { say("the app's window isn't under the press; nothing pressed"); return 3 }
        through = Set(over[..<mine].map(\.number))
    }
    func clear(_ p: CGPoint) -> Bool {
        guard let pids else { return true }
        guard let top = look.stack(p).first(where: { !through.contains($0.number) }) else { return false }
        return top.pid == pids.press || top.pid == pids.drop
    }
    let steps = 40
    let path = (0...steps).map { i -> CGPoint in
        let t = Double(i) / Double(steps)
        return CGPoint(x: from.x + (to.x - from.x) * t, y: from.y + (to.y - from.y) * t)
    }
    var at = 0
    func cancel(_ why: String) -> Int32 {
        say("\(why); drag cancelled")
        post(.escape); wait(0.2)
        while at > 0, clear(path[at - 1]) {
            at -= 1
            post(.dragged(path[at])); wait(0.016)
        }
        post(.up(path[at])); wait(0.3)
        return 3
    }
    // The same point twice is a click, as quick as a hand's: anything longer
    // is a hold, which picks a panel up in the dock.
    if from == to {
        post(.down(from)); wait(0.06)
        post(.up(from)); wait(0.3)
        return 0
    }
    // How long the button's down before it moves. RD_PRESS is for a press that has
    // to move before something's hold timer goes off.
    post(.down(from)); wait(press)
    for i in 1...steps {
        let p = path[i]
        guard clear(p) else { return cancel("something came up at (\(Int(p.x)), \(Int(p.y)))") }
        post(.dragged(p)); at = i
        wait(0.016)
    }
    wait(hold)
    shoot()
    if let pids, look.owner(to) != pids.drop { return cancel("the drop isn't the app's any more") }
    post(.up(to)); wait(0.6)
    return 0
}

#if !TESTING
let a = CommandLine.arguments
let env = ProcessInfo.processInfo.environment

/// Runs the guard program with a deadline; its pid only if it exited cleanly.
func owner(_ p: CGPoint) -> Int32? {
    guard let tool = env["RD_GUARD"] else { return nil }
    let run = Process(), out = Pipe()
    run.executableURL = URL(fileURLWithPath: tool)
    run.arguments = [String(Int(p.x.rounded())), String(Int(p.y.rounded()))]
    run.standardOutput = out
    let done = DispatchSemaphore(value: 0)
    run.terminationHandler = { _ in done.signal() }
    guard (try? run.run()) != nil else { return nil }
    if done.wait(timeout: .now() + 2) == .timedOut { run.terminate(); return nil }
    guard run.terminationReason == .exit, run.terminationStatus == 0 else { return nil }
    let said = String(data: out.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
    return Int32(said.split(separator: " ").first ?? "")
}
/// The same windows window-at looks through: on screen, not see-through, not
/// Notification Centre's whole-screen sentinel.
func stack(_ p: CGPoint) -> [(number: Int, pid: Int32)] {
    let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
    return list.compactMap { w in
        guard let b = w[kCGWindowBounds as String] as? [String: Double], (w[kCGWindowAlpha as String] as? Double ?? 1) > 0,
              (w[kCGWindowName as String] as? String) != "Notification Center",
              let x = b["X"], let y = b["Y"], let wd = b["Width"], let h = b["Height"],
              CGRect(x: x, y: y, width: wd, height: h).contains(p),
              let pid = w[kCGWindowOwnerPID as String] as? Int32 else { return nil }
        return (w[kCGWindowNumber as String] as? Int ?? -1, pid)
    }
}
func send(_ input: Input) {
    func mouse(_ type: CGEventType, _ p: CGPoint) {
        CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: p, mouseButton: .left)?.post(tap: .cghidEventTap)
    }
    switch input {
    case .move(let p): mouse(.mouseMoved, p)
    case .down(let p): mouse(.leftMouseDown, p)
    case .dragged(let p): mouse(.leftMouseDragged, p)
    case .up(let p): mouse(.leftMouseUp, p)
    case .escape:
        for down in [true, false] { CGEvent(keyboardEventSource: nil, virtualKey: 53, keyDown: down)?.post(tap: .cghidEventTap) }
    }
}
var pids: Pids?
if env["RD_GUARD"] != nil, let press = env["RD_PRESS_PID"].flatMap({ Int32($0) }) {
    pids = Pids(press: press, drop: env["RD_DROP_PID"].flatMap { Int32($0) } ?? press)
}
let shot = a[6]
exit(drag(
    from: CGPoint(x: Double(a[1])!, y: Double(a[2])!), to: CGPoint(x: Double(a[3])!, y: Double(a[4])!),
    hold: Double(a[5])!, press: Double(env["RD_PRESS"] ?? "") ?? 0.12, pids: pids,
    look: Look(owner: owner, stack: stack), post: send, wait: { Thread.sleep(forTimeInterval: $0) },
    shoot: {
        guard !shot.isEmpty, let id = env["RD_WINDOW"] else { return }
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
        p.arguments = ["-x", "-o", "-l", id, shot]
        try? p.run(); p.waitUntilExit()
    },
    say: { fputs("realdrag: \($0)\n", stderr) }))
#endif
