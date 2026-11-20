// Prints what macOS reports about HDR headroom ("EDR") on each screen, and the display's own idea of
// its limits (HID input report 6 of the Monitor Control collection: HDR peak / SDR maximum, in nits).

import AppKit
import IOKit.hid

for screen in NSScreen.screens {
    print("screen \(screen.localizedName): edrNow=\(screen.maximumExtendedDynamicRangeColorComponentValue) edrPotential=\(screen.maximumPotentialExtendedDynamicRangeColorComponentValue) edrReference=\(screen.maximumReferenceExtendedDynamicRangeColorComponentValue)")
}

let manager = IOHIDManagerCreate(kCFAllocatorDefault, IOOptionBits(kIOHIDOptionsTypeNone))
IOHIDManagerSetDeviceMatching(manager, [kIOHIDVendorIDKey: 0x05ac, kIOHIDProductIDKey: 0x1116, kIOHIDDeviceUsagePageKey: 0x80] as CFDictionary)
IOHIDManagerOpen(manager, IOOptionBits(kIOHIDOptionsTypeNone))
for device in (IOHIDManagerCopyDevices(manager) as? Set<IOHIDDevice>) ?? [] {
    let usage = IOHIDDeviceGetProperty(device, kIOHIDPrimaryUsageKey as CFString) as? Int ?? -1
    var feature = [UInt8](repeating: 0, count: 16)
    var length = feature.count
    if IOHIDDeviceGetReport(device, kIOHIDReportTypeFeature, 1, &feature, &length) == kIOReturnSuccess {
        print("hid usage \(usage) feature 1: \(feature.prefix(length).map { String(format: "%02x", $0) }.joined(separator: " "))")
    }
    var input = [UInt8](repeating: 0, count: 16)
    length = input.count
    if IOHIDDeviceGetReport(device, kIOHIDReportTypeInput, 6, &input, &length) == kIOReturnSuccess {
        print("hid usage \(usage) input 6: \(input.prefix(length).map { String(format: "%02x", $0) }.joined(separator: " "))")
    }
}
