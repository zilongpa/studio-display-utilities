// Companion to measure-mac.swift: how brightness changes over time on macOS.
//
//   swift measure-mac-dynamics.swift ramp     how the backlight moves after the slider is set (20 ms samples)
//   swift measure-mac-dynamics.swift auto     slider sweep with automatic brightness left on
//
// Everything is put back afterwards.

import CoreGraphics
import Foundation
import IOKit
import IOKit.hid

let mode = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "ramp"

guard let services = dlopen("/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices", RTLD_NOW) else { exit(1) }
func symbol<T>(_ name: String, _ type: T.Type) -> T? {
    guard let pointer = dlsym(services, name) else { return nil }
    return unsafeBitCast(pointer, to: type)
}
typealias GetFloat = @convention(c) (CGDirectDisplayID, UnsafeMutablePointer<Float>) -> Int32
typealias SetFloat = @convention(c) (CGDirectDisplayID, Float) -> Int32
typealias CanChange = @convention(c) (CGDirectDisplayID) -> Bool
typealias GetBool = @convention(c) (CGDirectDisplayID, UnsafeMutablePointer<Bool>) -> Int32
typealias SetBool = @convention(c) (CGDirectDisplayID, Bool) -> Int32
let canChange = symbol("DisplayServicesCanChangeBrightness", CanChange.self)!
let getBrightness = symbol("DisplayServicesGetBrightness", GetFloat.self)!
let setBrightness = symbol("DisplayServicesSetBrightness", SetFloat.self)!
let autoEnabled = symbol("DisplayServicesAmbientLightCompensationEnabled", GetBool.self)!
let enableAuto = symbol("DisplayServicesEnableAmbientLightCompensation", SetBool.self)!

var ids = [CGDirectDisplayID](repeating: 0, count: 16)
var count: UInt32 = 0
CGGetOnlineDisplayList(16, &ids, &count)
guard let display = ids.prefix(Int(count)).first(where: { canChange($0) }) else { print("# no display"); exit(2) }

let manager = IOHIDManagerCreate(kCFAllocatorDefault, IOOptionBits(kIOHIDOptionsTypeNone))
IOHIDManagerSetDeviceMatching(manager, [kIOHIDVendorIDKey: 0x05ac, kIOHIDProductIDKey: 0x1116, kIOHIDDeviceUsagePageKey: 0x80, kIOHIDDeviceUsageKey: 1] as CFDictionary)
IOHIDManagerOpen(manager, IOOptionBits(kIOHIDOptionsTypeNone))
guard let hid = (IOHIDManagerCopyDevices(manager) as? Set<IOHIDDevice>)?.first else { print("# no HID device"); exit(2) }

func nits() -> Double {
    var report = [UInt8](repeating: 0, count: 7)
    var length = report.count
    guard IOHIDDeviceGetReport(hid, kIOHIDReportTypeFeature, 1, &report, &length) == kIOReturnSuccess else { return -1 }
    return Double(UInt32(report[1]) | UInt32(report[2]) << 8 | UInt32(report[3]) << 16 | UInt32(report[4]) << 24) / 100
}
func slider() -> Float {
    var value: Float = -1
    _ = getBrightness(display, &value)
    return value
}

let original = slider()
var wasAuto = false
_ = autoEnabled(display, &wasAuto)
print("# original slider=\(original) auto=\(wasAuto) nits=\(nits())")

func ramp(from: Float, to: Float) {
    _ = setBrightness(display, from)
    Thread.sleep(forTimeInterval: 2)
    print("# ramp \(from) -> \(to), start nits=\(nits())")
    print("ms,nits")
    let start = Date()
    _ = setBrightness(display, to)
    var last = -1.0
    while Date().timeIntervalSince(start) < 2.0 {
        let now = nits()
        if now != last {
            print("\(Int(Date().timeIntervalSince(start) * 1000)),\(now)")
            last = now
        }
        Thread.sleep(forTimeInterval: 0.02)
    }
}

if mode == "ramp" {
    _ = enableAuto(display, false)
    Thread.sleep(forTimeInterval: 1)
    ramp(from: 0.5, to: 0.5625)   // one brightness key step up
    ramp(from: 0.5625, to: 0.5)   // one step down
    ramp(from: 0.25, to: 0.75)    // a large jump
} else {
    _ = enableAuto(display, true)
    Thread.sleep(forTimeInterval: 4)
    print("# auto on, settled: slider=\(slider()) nits=\(nits())")
    print("set,slider_readback,nits_after_2s,nits_after_6s")
    for step in stride(from: 16, through: 0, by: -2) {
        let value = Float(step) / 16
        _ = setBrightness(display, value)
        Thread.sleep(forTimeInterval: 2)
        let early = nits()
        Thread.sleep(forTimeInterval: 4)
        print("\(value),\(slider()),\(early),\(nits())")
    }
}

_ = setBrightness(display, original)
_ = enableAuto(display, wasAuto)
print("# restored")
