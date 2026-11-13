// Real mouse events for a drag test on Tinker Desk's desktop (never Mike's, as
// it takes the pointer: app/README.md, "A real drag"):
//   realdrag <fromX> <fromY> <toX> <toY> <holdSeconds> <shotPath>
// Screen points, from the top left. With a path, and a window number in
// RD_WINDOW, takes a picture of that window mid-drag before letting go.
//
// With RD_GUARD (a program that prints the pid of the window on top at
// `<x> <y>`, features/tools/real-screen.ts's window-at) and RD_PRESS_PID, it
// looks again just before pressing, after the hover, and presses nothing if
// that pid's window isn't the one there (exit 3). With RD_DROP_PID too, it
// looks at the drop just before letting go, and if something else has come
// up there it presses Escape first, so a drag in progress is cancelled
// rather than dropped on it (exit 3). The caller checked both just before
// starting this; these close the time the hover and the drag take.
import AppKit
import CoreGraphics

let a = CommandLine.arguments
let from = CGPoint(x: Double(a[1])!, y: Double(a[2])!)
let to = CGPoint(x: Double(a[3])!, y: Double(a[4])!)
let hold = Double(a[5])!
let shot = a[6]

func post(_ type: CGEventType, _ p: CGPoint) {
    CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: p, mouseButton: .left)?.post(tap: .cghidEventTap)
}
func wait(_ s: Double) { Thread.sleep(forTimeInterval: s) }
let env = ProcessInfo.processInfo.environment
func owner(_ p: CGPoint) -> Int32? {
    guard let guardTool = env["RD_GUARD"] else { return nil }
    let run = Process(), out = Pipe()
    run.executableURL = URL(fileURLWithPath: guardTool)
    run.arguments = [String(Int(p.x.rounded())), String(Int(p.y.rounded()))]
    run.standardOutput = out
    guard (try? run.run()) != nil else { return nil }
    run.waitUntilExit()
    let said = String(data: out.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
    return Int32(said.split(separator: " ").first ?? "")
}
/// Whether `p`'s top window belongs to the pid in `key`: true when there's no guard asked for.
func ours(_ p: CGPoint, _ key: String) -> Bool {
    guard env["RD_GUARD"] != nil, let want = env[key].flatMap({ Int32($0) }) else { return true }
    if owner(p) == want { return true }
    fputs("realdrag: the window at (\(Int(p.x)), \(Int(p.y))) isn't \(key)'s any more; \(key == "RD_PRESS_PID" ? "nothing pressed" : "drag cancelled")\n", stderr)
    return false
}
func escape() {
    for down in [true, false] { CGEvent(keyboardEventSource: nil, virtualKey: 53, keyDown: down)?.post(tap: .cghidEventTap); wait(0.05) }
}

post(.mouseMoved, from); wait(0.25)
if !ours(from, "RD_PRESS_PID") { exit(3) }
// The same point twice is a click, as quick as a hand's: anything longer
// is a hold, which picks a panel up in the dock.
if from == to {
    post(.leftMouseDown, from); wait(0.06)
    post(.leftMouseUp, from); wait(0.3)
    exit(0)
}
// How long the button's down before it moves. RD_PRESS is for a press that has
// to move before something's hold timer goes off.
post(.leftMouseDown, from); wait(Double(env["RD_PRESS"] ?? "") ?? 0.12)
let steps = 40
for i in 1...steps {
    let t = Double(i) / Double(steps)
    post(.leftMouseDragged, CGPoint(x: from.x + (to.x - from.x) * t, y: from.y + (to.y - from.y) * t))
    wait(0.016)
}
wait(hold)
if !shot.isEmpty, let id = env["RD_WINDOW"] {
    let p = Process()
    p.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
    p.arguments = ["-x", "-o", "-l", id, shot]
    try? p.run(); p.waitUntilExit()
}
if !ours(to, "RD_DROP_PID") {
    escape(); wait(0.2)
    post(.leftMouseUp, to); wait(0.3)
    exit(3)
}
post(.leftMouseUp, to); wait(0.6)
