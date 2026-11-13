// Draws the Swift app's line bear (app/Sources/Threadbear/BearGeometry.swift)
// out as SVGs, one per part that moves on its own, for the GPUI app's bears
// (desktop/gpui/src/bears.rs). The geometry stays in one place, Swift's, and
// this makes the pictures from it:
//
//   swiftc -parse-as-library -o /tmp/bear-svgs app/Sources/Threadbear/BearGeometry.swift desktop/scripts/bear-svgs.swift
//   /tmp/bear-svgs desktop/gpui/icons/bears
//
// GPUI turns and scales an SVG about the middle of its box, so each part's
// picture is a 48-unit square centred on the point it turns about (the feet,
// the shoulder, the hat's tip): drawn at a bear's side `s`, its box is `2s`
// across with that point in the middle (bears.rs `Part`). An ink that shows
// only outside another shape (an ear behind the head) keeps that as an
// even-odd clip, as resvg draws it.
import CoreGraphics
import Foundation

func number(_ v: CGFloat) -> String {
    let r = (v * 1000).rounded() / 1000
    return r == r.rounded() ? String(Int(r)) : String(format: "%g", Double(r))
}

func pathData(_ path: CGPath) -> String {
    var d = ""
    path.applyWithBlock { element in
        let e = element.pointee
        let p = e.points
        func pt(_ i: Int) -> String { "\(number(p[i].x)) \(number(p[i].y))" }
        switch e.type {
        case .moveToPoint: d += "M\(pt(0))"
        case .addLineToPoint: d += "L\(pt(0))"
        case .addQuadCurveToPoint: d += "Q\(pt(0)) \(pt(1))"
        case .addCurveToPoint: d += "C\(pt(0)) \(pt(1)) \(pt(2))"
        case .closeSubpath: d += "Z"
        @unknown default: break
        }
    }
    return d
}

/// The inks as an SVG whose 48-unit square is centred on `about`.
func svg(_ inks: [BearGeometry.Ink], about: CGPoint) -> String {
    var defs = "", body = ""
    for (i, ink) in inks.enumerated() {
        var clip = ""
        if let outside = ink.outside {
            defs += "<clipPath id=\"c\(i)\"><path clip-rule=\"evenodd\" d=\"M-24 -24H48V72H-24Z\(pathData(outside))\"/></clipPath>"
            clip = " clip-path=\"url(#c\(i))\""
        }
        let paint = ink.width.map { "fill=\"none\" stroke=\"black\" stroke-width=\"\(number($0))\" stroke-linecap=\"round\" stroke-linejoin=\"round\"" } ?? "fill=\"black\""
        body += "<path d=\"\(pathData(ink.path))\" \(paint)\(clip)/>"
    }
    let box = "\(number(about.x - 24)) \(number(about.y - 24)) 48 48"
    return "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"\(box)\">" + (defs.isEmpty ? "" : "<defs>\(defs)</defs>") + body + "</svg>\n"
}

/// A shape filled, for covering what's behind it in the background's colour.
func filled(_ path: CGPath) -> [BearGeometry.Ink] { [BearGeometry.Ink(path: path, width: nil)] }

@main
enum BearSVGs {
    static func main() throws {
        let out = URL(fileURLWithPath: CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "desktop/gpui/icons/bears")
        try FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)

        let bottom = CGPoint(x: 12, y: 24)
        let big = BearGeometry.waveFace(small: false)
        let small = BearGeometry.waveFace(small: true)
        var parts: [(String, [BearGeometry.Ink], CGPoint)] = [
            // Whole poses, turning about the bottom of their square (SwiftUI's `.bottom` anchor).
            ("wave", BearGeometry.inks(.wave, side: 44), bottom),
            ("wave-small", BearGeometry.inks(.wave, side: 18), bottom),
            ("sit", BearGeometry.inks(.sit, side: 24), bottom),
            ("cub", BearGeometry.inks(.cub, side: 24), bottom),
            // The working bear in parts: the paw swings about the shoulder from
            // behind the head (the head's filled in the background's colour over it),
            // the left eye blinks, and the lot rocks and breathes about its feet.
            ("work-arm", [BearGeometry.Ink(path: BearGeometry.arm, width: BearGeometry.line)] + (BearGeometry.pawPad(small: false).map { [BearGeometry.Ink(path: $0, width: nil)] } ?? []), BearGeometry.shoulder),
            ("work-head-fill", filled(big.head), BearGeometry.feet),
            ("work-face", big.stillInks, BearGeometry.feet),
            ("work-eye", big.eyeInks, CGPoint(x: big.centre.x + (8 - 12) * big.scale, y: big.centre.y + (11.6 - 13.2) * big.scale)),
            ("work-face-small", small.stillInks, BearGeometry.feet),
            // Asleep: the head, and each Z on its own to drift.
            ("sleep-head", BearGeometry.sleepingHead(small: false), BearGeometry.sleepingFeet),
            ("sleep-z1", [BearGeometry.Ink(path: BearGeometry.sleepZs[0], width: BearGeometry.Face.shutWidth)], BearGeometry.sleepingFeet),
            ("sleep-z2", [BearGeometry.Ink(path: BearGeometry.sleepZs[1], width: BearGeometry.Face.shutWidth)], BearGeometry.sleepingFeet),
        ]
        // Waking up: the sat bear with its eyes shut, then open, and the arm it stretches.
        let sitShut = BearGeometry.sitFace(small: true, eyes: .shut)
        let sitOpen = BearGeometry.sitFace(small: true)
        parts += [
            ("sit-shut", BearGeometry.sitBody(face: sitShut) + sitShut.inks, BearGeometry.sitFeet),
            ("sit-still", BearGeometry.sitBody(face: sitOpen) + sitOpen.stillInks, BearGeometry.sitFeet),
            ("sit-eye", sitOpen.eyeInks, BearGeometry.sitFeet),
            ("sit-fill", filled(BearGeometry.sitOutline(hat: false)), BearGeometry.sitFeet),
            ("stretch-arm", [BearGeometry.Ink(path: BearGeometry.stretchArm, width: BearGeometry.line)], BearGeometry.stretchShoulder),
            ("sleep-head-small", BearGeometry.sleepingHead(small: true), BearGeometry.sleepingFeet),
        ]

        for (name, inks, about) in parts {
            try svg(inks, about: about).write(to: out.appendingPathComponent("\(name).svg"), atomically: true, encoding: .utf8)
        }
        print("wrote \(parts.count) bears to \(out.path)")
    }
}
