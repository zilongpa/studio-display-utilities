// Reads or switches the "High Dynamic Range" setting of the display on macOS (private SkyLight API).
//
//   swift mac-hdr.swift          print the state
//   swift mac-hdr.swift on|off   switch it

import CoreGraphics
import Foundation

guard let skylight = dlopen("/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight", RTLD_NOW),
      let services = dlopen("/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices", RTLD_NOW) else { exit(1) }
typealias Query = @convention(c) (CGDirectDisplayID) -> Bool
typealias Switch = @convention(c) (CGDirectDisplayID, Bool) -> Int32
func query(_ name: String) -> Query? { dlsym(skylight, name).map { unsafeBitCast($0, to: Query.self) } }
let supports = query("SLSDisplaySupportsHDRMode")
let enabled = query("SLSDisplayIsHDRModeEnabled")
let set = dlsym(skylight, "SLSDisplaySetHDRModeEnabled").map { unsafeBitCast($0, to: Switch.self) }
let canChange = dlsym(services, "DisplayServicesCanChangeBrightness").map { unsafeBitCast($0, to: Query.self) }
print("# symbols: supports=\(supports != nil) enabled=\(enabled != nil) set=\(set != nil)")

var ids = [CGDirectDisplayID](repeating: 0, count: 16)
var count: UInt32 = 0
CGGetOnlineDisplayList(16, &ids, &count)
for id in ids.prefix(Int(count)) {
    let studio = canChange?(id) ?? false
    print("display \(id): \(CGDisplayPixelsWide(id))x\(CGDisplayPixelsHigh(id)) brightnessControl=\(studio) supportsHDR=\(supports.map { String($0(id)) } ?? "?") hdrEnabled=\(enabled.map { String($0(id)) } ?? "?")")
    if studio, CommandLine.arguments.count > 1, let set = set {
        let want = CommandLine.arguments[1] == "on"
        let result = set(id, want)
        Thread.sleep(forTimeInterval: 4)
        print("set HDR \(want): result=\(result) now hdrEnabled=\(enabled.map { String($0(id)) } ?? "?")")
    }
}
