// Measures how macOS maps its brightness slider to nits on the Studio Display XDR.
//
// Run on a Mac that drives the display:   swift measure-mac.swift [steps]
//
// For each slider position (0 ... 1) it sets the position through the same private API the
// brightness keys and the Displays settings use, then reads the backlight in nits straight from the
// display over USB HID (feature report 1, 0.01 nit units). Automatic brightness is switched off for
// the sweep and everything is put back afterwards. Output is CSV: slider,linear,nits

import CoreGraphics
import Foundation
import IOKit
import IOKit.hid

let steps = CommandLine.arguments.count > 1 ? max(2, Int(CommandLine.arguments[1]) ?? 64) : 64

guard let services = dlopen("/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices", RTLD_NOW) else {
    print("DisplayServices could not be loaded"); exit(1)
}
func symbol<T>(_ name: String, _ type: T.Type) -> T? {
    guard let pointer = dlsym(services, name) else { return nil }
    return unsafeBitCast(pointer, to: type)
}
typealias GetFloat = @convention(c) (CGDirectDisplayID, UnsafeMutablePointer<Float>) -> Int32
typealias SetFloat = @convention(c) (CGDirectDisplayID, Float) -> Int32
typealias CanChange = @convention(c) (CGDirectDisplayID) -> Bool
typealias GetBool = @convention(c) (CGDirectDisplayID, UnsafeMutablePointer<Bool>) -> Int32
typealias SetBool = @convention(c) (CGDirectDisplayID, Bool) -> Int32

let canChange = symbol("DisplayServicesCanChangeBrightness", CanChange.self)
let getBrightness = symbol("DisplayServicesGetBrightness", GetFloat.self)
let setBrightness = symbol("DisplayServicesSetBrightness", SetFloat.self)
let getLinear = symbol("DisplayServicesGetLinearBrightness", GetFloat.self)
let autoEnabled = symbol("DisplayServicesAmbientLightCompensationEnabled", GetBool.self)
let enableAuto = symbol("DisplayServicesEnableAmbientLightCompensation", SetBool.self)
print("# symbols: canChange=\(canChange != nil) get=\(getBrightness != nil) set=\(setBrightness != nil) linear=\(getLinear != nil) autoGet=\(autoEnabled != nil) autoSet=\(enableAuto != nil)")

// The display to measure: the first one whose brightness macOS can change.
var ids = [CGDirectDisplayID](repeating: 0, count: 16)
var count: UInt32 = 0
CGGetOnlineDisplayList(16, &ids, &count)
var display: CGDirectDisplayID?
for id in ids.prefix(Int(count)) {
    let able = canChange?(id) ?? false
    print("# display \(id): vendor=0x\(String(CGDisplayVendorNumber(id), radix: 16)) model=0x\(String(CGDisplayModelNumber(id), radix: 16)) \(CGDisplayPixelsWide(id))x\(CGDisplayPixelsHigh(id)) canChangeBrightness=\(able)")
    if able && display == nil { display = id }
}

// The display's backlight over HID, for the real value in nits.
let manager = IOHIDManagerCreate(kCFAllocatorDefault, IOOptionBits(kIOHIDOptionsTypeNone))
IOHIDManagerSetDeviceMatching(manager, [kIOHIDVendorIDKey: 0x05ac, kIOHIDProductIDKey: 0x1116, kIOHIDDeviceUsagePageKey: 0x80, kIOHIDDeviceUsageKey: 1] as CFDictionary)
IOHIDManagerOpen(manager, IOOptionBits(kIOHIDOptionsTypeNone))
let hid = (IOHIDManagerCopyDevices(manager) as? Set<IOHIDDevice>)?.first
print("# HID backlight device found: \(hid != nil)")

func nits() -> Double? {
    guard let hid = hid else { return nil }
    var report = [UInt8](repeating: 0, count: 7)
    var length = report.count
    guard IOHIDDeviceGetReport(hid, kIOHIDReportTypeFeature, 1, &report, &length) == kIOReturnSuccess, length >= 5 else { return nil }
    return Double(UInt32(report[1]) | UInt32(report[2]) << 8 | UInt32(report[3]) << 16 | UInt32(report[4]) << 24) / 100
}

guard let display = display, let getBrightness = getBrightness, let setBrightness = setBrightness else {
    print("# no display whose brightness macOS can change - is the Studio Display connected to this Mac?")
    exit(2)
}

var original: Float = 0
_ = getBrightness(display, &original)
var wasAuto = false
let knowAuto = autoEnabled.map { $0(display, &wasAuto) == 0 } ?? false
print("# original slider=\(original) autoBrightness=\(knowAuto ? String(wasAuto) : "unknown") nits=\(nits().map { String($0) } ?? "n/a")")
if knowAuto && wasAuto { _ = enableAuto?(display, false); Thread.sleep(forTimeInterval: 2) }

print("slider,linear,nits")
for step in 0...steps {
    let slider = Float(step) / Float(steps)
    _ = setBrightness(display, slider)
    Thread.sleep(forTimeInterval: 0.8) // let the fade finish
    var linear: Float = -1
    _ = getLinear?(display, &linear)
    print("\(slider),\(linear),\(nits().map { String($0) } ?? "")")
}

_ = setBrightness(display, original)
if knowAuto && wasAuto { _ = enableAuto?(display, true) }
print("# restored")
