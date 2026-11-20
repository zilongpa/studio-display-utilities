// Resets macOS' automatic brightness curve for the display to its default, by switching automatic
// brightness off and on again (macOS forgets the user's adjustments when it is switched on).

import CoreGraphics
import Foundation

guard let services = dlopen("/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices", RTLD_NOW) else { exit(1) }
typealias CanChange = @convention(c) (CGDirectDisplayID) -> Bool
typealias GetBool = @convention(c) (CGDirectDisplayID, UnsafeMutablePointer<Bool>) -> Int32
typealias SetBool = @convention(c) (CGDirectDisplayID, Bool) -> Int32
let canChange = unsafeBitCast(dlsym(services, "DisplayServicesCanChangeBrightness"), to: CanChange.self)
let autoEnabled = unsafeBitCast(dlsym(services, "DisplayServicesAmbientLightCompensationEnabled"), to: GetBool.self)
let enableAuto = unsafeBitCast(dlsym(services, "DisplayServicesEnableAmbientLightCompensation"), to: SetBool.self)

var ids = [CGDirectDisplayID](repeating: 0, count: 16)
var count: UInt32 = 0
CGGetOnlineDisplayList(16, &ids, &count)
guard let display = ids.prefix(Int(count)).first(where: { canChange($0) }) else { print("no display"); exit(2) }

var state = false
_ = autoEnabled(display, &state)
print("automatic brightness before: \(state)")
print("off: \(enableAuto(display, false))")
Thread.sleep(forTimeInterval: 2)
print("on: \(enableAuto(display, true))")
Thread.sleep(forTimeInterval: 3)
_ = autoEnabled(display, &state)
print("automatic brightness after: \(state)")
