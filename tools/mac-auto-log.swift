// Records what macOS' automatic brightness does on the Studio Display XDR while the room light changes.
//
//   swift mac-auto-log.swift [seconds]
//
// Changes nothing. Every 100 ms it reads the ambient light (as macOS sees it), the brightness slider, and the backlight in nits, and prints a CSV line whenever one of
// them changed (and at least every 5 seconds).

import CoreGraphics
import Foundation
import IOKit
import IOKit.hid

let seconds = CommandLine.arguments.count > 1 ? Double(CommandLine.arguments[1]) ?? 180 : 180

guard let services = dlopen("/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices", RTLD_NOW) else { exit(1) }
typealias GetFloat = @convention(c) (CGDirectDisplayID, UnsafeMutablePointer<Float>) -> Int32
typealias CanChange = @convention(c) (CGDirectDisplayID) -> Bool
typealias GetBool = @convention(c) (CGDirectDisplayID, UnsafeMutablePointer<Bool>) -> Int32
let canChange = unsafeBitCast(dlsym(services, "DisplayServicesCanChangeBrightness"), to: CanChange.self)
let getBrightness = unsafeBitCast(dlsym(services, "DisplayServicesGetBrightness"), to: GetFloat.self)
let autoEnabled = unsafeBitCast(dlsym(services, "DisplayServicesAmbientLightCompensationEnabled"), to: GetBool.self)

var ids = [CGDirectDisplayID](repeating: 0, count: 16)
var count: UInt32 = 0
CGGetOnlineDisplayList(16, &ids, &count)
guard let display = ids.prefix(Int(count)).first(where: { canChange($0) }) else { print("# no display"); exit(2) }

let manager = IOHIDManagerCreate(kCFAllocatorDefault, IOOptionBits(kIOHIDOptionsTypeNone))
IOHIDManagerSetDeviceMatching(manager, [kIOHIDVendorIDKey: 0x05ac, kIOHIDProductIDKey: 0x1116] as CFDictionary)
IOHIDManagerOpen(manager, IOOptionBits(kIOHIDOptionsTypeNone))
let devices = Array((IOHIDManagerCopyDevices(manager) as? Set<IOHIDDevice>) ?? [])
func device(page: Int) -> IOHIDDevice? {
    devices.first { (IOHIDDeviceGetProperty($0, kIOHIDPrimaryUsagePageKey as CFString) as? Int) == page }
}
let backlight = device(page: 0x80)
print("# devices: \(devices.map { "\(IOHIDDeviceGetProperty($0, kIOHIDPrimaryUsagePageKey as CFString) as? Int ?? -1):\(IOHIDDeviceGetProperty($0, kIOHIDPrimaryUsageKey as CFString) as? Int ?? -1)" })")
print("# backlight=\(backlight != nil)")

func nits() -> Double {
    guard let backlight = backlight else { return -1 }
    var report = [UInt8](repeating: 0, count: 7)
    var length = report.count
    guard IOHIDDeviceGetReport(backlight, kIOHIDReportTypeFeature, 1, &report, &length) == kIOReturnSuccess else { return -1 }
    return Double(UInt32(report[1]) | UInt32(report[2]) << 8 | UInt32(report[3]) << 16 | UInt32(report[4]) << 24) / 100
}

// Ambient light as macOS itself sees it: the sensor service of the display in the HID event system
// (private API; the display's light sensor is not readable as a plain HID device on macOS).
@_silgen_name("IOHIDEventSystemClientCreate") func IOHIDEventSystemClientCreate(_ allocator: CFAllocator?) -> AnyObject?
@_silgen_name("IOHIDEventSystemClientCopyServices") func IOHIDEventSystemClientCopyServices(_ client: AnyObject) -> CFArray?
@_silgen_name("IOHIDServiceClientCopyEvent") func IOHIDServiceClientCopyEvent(_ service: AnyObject, _ type: Int64, _ options: Int32, _ timestamp: Int64) -> AnyObject?
@_silgen_name("IOHIDServiceClientCopyProperty") func IOHIDServiceClientCopyProperty(_ service: AnyObject, _ key: CFString) -> AnyObject?
@_silgen_name("IOHIDEventGetFloatValue") func IOHIDEventGetFloatValue(_ event: AnyObject, _ field: Int32) -> Double
let ambientLightEvent: Int64 = 12
let eventClient = IOHIDEventSystemClientCreate(kCFAllocatorDefault)
let lightServices = ((eventClient.flatMap { IOHIDEventSystemClientCopyServices($0) } as? [AnyObject]) ?? []).filter {
    (IOHIDServiceClientCopyProperty($0, "VendorID" as CFString) as? Int) == 0x05ac && IOHIDServiceClientCopyEvent($0, ambientLightEvent, 0, 0) != nil
}
print("# ambient light services: \(lightServices.count)")
func lux(_ index: Int) -> Double {
    guard index < lightServices.count, let event = IOHIDServiceClientCopyEvent(lightServices[index], ambientLightEvent, 0, 0) else { return -1 }
    return IOHIDEventGetFloatValue(event, Int32(ambientLightEvent << 16))
}

var isAuto = false
_ = autoEnabled(display, &isAuto)
print("# automatic brightness: \(isAuto)")
print("seconds,lux,lux_second_sensor,slider,nits")

let start = Date()
var lastLine = ""
var lastPrinted = -10.0
while Date().timeIntervalSince(start) < seconds {
    var slider: Float = -1
    _ = getBrightness(display, &slider)
    let line = String(format: "%.1f,%.1f,%.4f,%.2f", lux(0), lux(1), slider, nits())
    let elapsed = Date().timeIntervalSince(start)
    if line != lastLine || elapsed - lastPrinted >= 5 {
        print(String(format: "%.1f,", elapsed) + line)
        fflush(stdout)
        lastLine = line
        lastPrinted = elapsed
    }
    Thread.sleep(forTimeInterval: 0.1)
}
print("# done")
