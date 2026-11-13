// Appended to app/Tools/realdrag.swift (built with -D TESTING): the drag's
// decisions against a made-up screen, recording what it would post.
import CoreGraphics

let app: Int32 = 100, target: Int32 = 555, alert: Int32 = 200, dock: Int32 = 10
let from = CGPoint(x: 400, y: 300), to = CGPoint(x: 800, y: 300)

struct Run { var code: Int32; var sent: [Input] }
/// `owner` and `stack` answer from the step count so far: a window can come up mid-drag.
func run(_ f: CGPoint = from, _ t: CGPoint = to, pids: Pids? = Pids(press: app, drop: app),
         owner: @escaping (CGPoint, Int) -> Int32? = { _, _ in app },
         stack: @escaping (CGPoint, Int) -> [(number: Int, pid: Int32)] = { _, _ in [(1, app)] }) -> Run {
    var sent: [Input] = []
    var dragged = 0
    let code = drag(from: f, to: t, hold: 0, press: 0, pids: pids,
                    look: Look(owner: { owner($0, dragged) }, stack: { stack($0, dragged) }),
                    post: { if case .dragged = $0 { dragged += 1 }; sent.append($0) }, wait: { _ in }, shoot: {}, say: { _ in })
    return Run(code: code, sent: sent)
}
func check(_ ok: Bool, _ label: String) { precondition(ok, label) }
func ups(_ r: Run) -> [CGPoint] { r.sent.compactMap { if case .up(let p) = $0 { return p } else { return nil } } }
let pressed = { (r: Run) in r.sent.contains { if case .down = $0 { return true } else { return false } } }

// Unguarded, as the other tools use it: the same events as ever.
var r = run(pids: nil, owner: { _, _ in nil }, stack: { _, _ in [] })
check(r.code == 0 && r.sent.count == 43 && r.sent.first == .move(from) && r.sent.last == .up(to), "unguarded drag unchanged")
// Guarded and clear all the way: the same events.
r = run()
check(r.code == 0 && r.sent.count == 43 && r.sent.last == .up(to) && !r.sent.contains(.escape), "a clear drag")
// The press point changed hands during the hover: only the move went out.
r = run(owner: { _, _ in alert })
check(r.code == 3 && r.sent == [.move(from)], "press refused after the hover")
// The guard program failed or hung (nil): the same.
r = run(owner: { _, _ in nil })
check(r.code == 3 && r.sent == [.move(from)], "guard failure presses nothing")
// A click: looked at before the press.
r = run(from, from, owner: { _, _ in alert })
check(r.code == 3 && !pressed(r), "click refused")
r = run(from, from)
check(r.code == 0 && r.sent == [.move(from), .down(from), .up(from)], "a click")
// An alert comes up over the path after 10 steps: Escape, back along the
// path to the press point, released there; never a step or release on the alert.
let alertFrom = { (p: CGPoint, n: Int) -> [(number: Int, pid: Int32)] in n >= 10 && p.x > 650 ? [(7, alert), (1, app)] : [(1, app)] }
r = run(stack: alertFrom)
check(r.code == 3 && r.sent.contains(.escape) && ups(r) == [from], "cancel goes back to the press point")
check(!r.sent.contains { if case .dragged(let p) = $0 { return p.x > 650 } else { return false } }, "no step over the alert")
// Back blocked too (something came up behind): released at the last clear step, which was the app's.
r = run(stack: { p, n in n >= 10 && (p.x > 650 || p.x < 480) ? [(7, alert), (1, app)] : [(1, app)] })
check(r.code == 3 && ups(r).count == 1 && ups(r)[0].x >= 480 && ups(r)[0].x <= 650, "released at the last clear step")
// The drop's owner changed (or the guard failed) at the end: cancelled, back to the press point.
r = run(owner: { p, _ in p == to ? nil : app })
check(r.code == 3 && r.sent.contains(.escape) && ups(r) == [from], "drop refused")
// A drop on our own target beside the app: steps over either are fine.
r = run(pids: Pids(press: app, drop: target), owner: { p, _ in p.x >= 600 ? target : app }, stack: { p, _ in p.x >= 600 ? [(5, target), (1, app)] : [(1, app)] })
check(r.code == 0 && r.sent.last == .up(to), "drag to our drop target")
// The Dock's overlay over everything, let through at the press: still let through.
r = run(stack: { _, _ in [(9, dock), (1, app)] })
check(r.code == 0, "overlay already over the press is let through")
// But a different overlay window, even the same app's, is not.
r = run(stack: { p, _ in p.x > 600 ? [(8, dock), (1, app)] : [(9, dock), (1, app)] })
check(r.code == 3, "a new overlay window is not")
// The app's window not under the press at all: nothing pressed.
r = run(stack: { _, _ in [(7, alert)] })
check(r.code == 3 && !pressed(r), "no app window under the press")
print("realdrag checks passed")
