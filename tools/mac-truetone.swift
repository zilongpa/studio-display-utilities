// True Tone experiment on a Mac driving a Studio Display: finds out where True Tone happens and how
// the screen's white point follows the ambient light.
//
//   swift mac-truetone.swift snapshot <label>   write everything readable to tt-<label>.json
//   swift mac-truetone.swift toggle             True Tone off / on at constant light, report what differs
//   swift mac-truetone.swift log <seconds> [guide]
//                                               record 5 times a second to tt-log.jsonl;
//                                               with "guide", speak the lamp steps of the experiment
//   swift mac-truetone.swift settings           same light, vary screen brightness and HDR
//
// What it reads: True Tone state and the colour adaptation values of the brightness service
// (CoreBrightness, private), the display's HID feature reports, the ambient light as macOS sees it,
// and the display's transfer (gamma) table. The display is only ever read from. "toggle" and
// "settings" change the True Tone switch, the brightness slider and the HDR switch, and put them
// back afterwards.

import CoreGraphics
import Foundation
import IOKit
import IOKit.hid

setvbuf(stdout, nil, _IONBF, 0)

// MARK: - CoreBrightness (private)

dlopen("/System/Library/PrivateFrameworks/CoreBrightness.framework/CoreBrightness", RTLD_NOW)

/// Property names of the brightness service that concern colour adaptation ("Harmony" is True Tone),
/// the colour ramps it applies, and the ambient light it works from. Found in the system libraries.
let propertyKeys = [
    "ColorAdaptationActive", "ColorAdaptationAvailable", "ColorAdaptationEnabled", "ColorAdaptationMode",
    "ColorAdaptationModeEnabled", "ColorAdaptationModeMapping", "ColorAdaptationStrength", "ColorAdaptationStrengthPeriod",
    "CBColorAdaptationEnabled", "ColorRamp", "ColorRampHarmony", "ColorRampAmmolite", "ColorRampBLR", "ColorRampInput",
    "ColorRampPeriod", "ColorRampCache", "ColorRampComponent", "AdaptationEnabled", "AdaptationLux", "AdaptationMtx",
    "AdaptationStrengthBins", "AmbientColor", "AmbientColorBins", "AmbientColorTransitionBins", "CBColorSample",
    "CBChromaticCorrection", "CBChromaticCorrectionParams", "AggregatedLux", "TrustedLux", "CBAmmoliteEnabled",
    "CBAmmoliteSupported", "AmmoliteStrength", "CBTwilight", "BlueLightReductionStatus", "CBBlueLightReductionCCTTargetRaw",
    "AutoBrightnessEnabled", "CBAutoBrightnessEnabled", "AutoBrightnessCurve", "DisplayBrightness", "CBDisplayBrightnessState",
]

final class Brightness {
    private let system: NSObject?
    private let trueTone: NSObject?

    init() {
        system = (NSClassFromString("BrightnessSystemClient") as? NSObject.Type)?.init()
        trueTone = (NSClassFromString("CBTrueToneClient") as? NSObject.Type)?.init()
    }

    func property(_ key: String) -> String? {
        let selector = NSSelectorFromString("copyPropertyForKey:")
        guard let system = system, system.responds(to: selector), let value = system.perform(selector, with: key)?.takeRetainedValue() else { return nil }
        return String(describing: value).replacingOccurrences(of: "\n", with: " ").replacingOccurrences(of: "    ", with: "")
    }

    private func flag(_ name: String) -> Bool? {
        let selector = NSSelectorFromString(name)
        guard let client = trueTone, client.responds(to: selector) else { return nil }
        typealias Getter = @convention(c) (AnyObject, Selector) -> Bool
        return unsafeBitCast(client.method(for: selector), to: Getter.self)(client, selector)
    }

    var trueToneSupported: Bool? { flag("supported") }
    var trueToneAvailable: Bool? { flag("available") }
    var trueToneEnabled: Bool? { flag("enabled") }

    func setTrueTone(_ on: Bool) -> Bool {
        let selector = NSSelectorFromString("setEnabled:")
        guard let client = trueTone, client.responds(to: selector) else { return false }
        typealias Setter = @convention(c) (AnyObject, Selector, Bool) -> Bool
        return unsafeBitCast(client.method(for: selector), to: Setter.self)(client, selector, on)
    }

    func all() -> [String: String] {
        var values: [String: String] = [:]
        for key in propertyKeys { if let value = property(key) { values[key] = value } }
        for (name, value) in [("TrueTone.supported", trueToneSupported), ("TrueTone.available", trueToneAvailable), ("TrueTone.enabled", trueToneEnabled)] {
            if let value = value { values[name] = String(value) }
        }
        return values
    }
}

// MARK: - The display: brightness slider, HDR switch, transfer table

let displayServices = dlopen("/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices", RTLD_NOW)
let skyLight = dlopen("/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight", RTLD_NOW)
func function<T>(_ library: UnsafeMutableRawPointer?, _ name: String, _ type: T.Type) -> T? {
    dlsym(library, name).map { unsafeBitCast($0, to: type) }
}
typealias GetFloat = @convention(c) (CGDirectDisplayID, UnsafeMutablePointer<Float>) -> Int32
typealias SetFloat = @convention(c) (CGDirectDisplayID, Float) -> Int32
typealias Query = @convention(c) (CGDirectDisplayID) -> Bool
typealias GetBool = @convention(c) (CGDirectDisplayID, UnsafeMutablePointer<Bool>) -> Int32
typealias SetBool = @convention(c) (CGDirectDisplayID, Bool) -> Int32
let canChangeBrightness = function(displayServices, "DisplayServicesCanChangeBrightness", Query.self)
let getSlider = function(displayServices, "DisplayServicesGetBrightness", GetFloat.self)
let setSlider = function(displayServices, "DisplayServicesSetBrightness", SetFloat.self)
let autoBrightnessOn = function(displayServices, "DisplayServicesAmbientLightCompensationEnabled", GetBool.self)
let setAutoBrightness = function(displayServices, "DisplayServicesEnableAmbientLightCompensation", SetBool.self)
let hdrOn = function(skyLight, "SLSDisplayIsHDRModeEnabled", Query.self)
let setHdr = function(skyLight, "SLSDisplaySetHDRModeEnabled", SetBool.self)

var onlineDisplays = [CGDirectDisplayID](repeating: 0, count: 16)
var onlineCount: UInt32 = 0
CGGetOnlineDisplayList(16, &onlineDisplays, &onlineCount)
/// The Studio Display: the first display whose brightness macOS can change.
let display = onlineDisplays.prefix(Int(onlineCount)).first { canChangeBrightness?($0) ?? false }

func slider() -> Float? {
    guard let display = display, let get = getSlider else { return nil }
    var value: Float = -1
    return get(display, &value) == 0 ? value : nil
}

/// The display's transfer table at a few input levels: [red, green, blue] each. A white point shift
/// applied through the table shows up as the three channels drifting apart.
func transferTable() -> [[Float]]? {
    guard let display = display else { return nil }
    var red = [Float](repeating: 0, count: 256), green = red, blue = red
    var count: UInt32 = 0
    guard CGGetDisplayTransferByTable(display, 256, &red, &green, &blue, &count) == .success, count > 1 else { return nil }
    let last = Int(count) - 1
    return [last / 4, last / 2, last * 3 / 4, last].map { [red[$0], green[$0], blue[$0]] }
}

// MARK: - The display's HID interfaces

let hidManager = IOHIDManagerCreate(kCFAllocatorDefault, IOOptionBits(kIOHIDOptionsTypeNone))
IOHIDManagerSetDeviceMatching(hidManager, [kIOHIDVendorIDKey: 0x05ac] as CFDictionary)
IOHIDManagerOpen(hidManager, IOOptionBits(kIOHIDOptionsTypeNone))

struct Interface {
    let device: IOHIDDevice
    let name: String          // "page:usage" of the top level collection
    let featureReports: [(id: Int, bytes: Int)]
}

/// Apple display HID interfaces (a keyboard or trackpad from Apple has no vendor display pages and
/// is told apart by the product id of the display found through its brightness collection).
let interfaces: [Interface] = {
    let devices = Array((IOHIDManagerCopyDevices(hidManager) as? Set<IOHIDDevice>) ?? [])
    func number(_ device: IOHIDDevice, _ key: String) -> Int { IOHIDDeviceGetProperty(device, key as CFString) as? Int ?? -1 }
    guard let monitor = devices.first(where: { number($0, kIOHIDPrimaryUsagePageKey) == 0x80 }) else { return [] }
    let product = number(monitor, kIOHIDProductIDKey)
    return devices.filter { number($0, kIOHIDProductIDKey) == product }.map { device in
        var ids = Set<Int>()
        for element in (IOHIDDeviceCopyMatchingElements(device, nil, IOOptionBits(kIOHIDOptionsTypeNone)) as? [IOHIDElement]) ?? []
        where IOHIDElementGetType(element) == kIOHIDElementTypeFeature {
            ids.insert(Int(IOHIDElementGetReportID(element)))
        }
        // The length of each report is taken from what the display actually returns for it.
        let reports: [(id: Int, bytes: Int)] = ids.sorted().map { id in
            var buffer = [UInt8](repeating: 0, count: 16384)
            var length = buffer.count
            let ok = IOHIDDeviceGetReport(device, kIOHIDReportTypeFeature, id, &buffer, &length) == kIOReturnSuccess
            return (id: id, bytes: ok ? length : 0)
        }
        let name = String(format: "%04x:%04x", number(device, kIOHIDPrimaryUsagePageKey), number(device, kIOHIDPrimaryUsageKey))
        return Interface(device: device, name: name, featureReports: reports)
    }.sorted { $0.name < $1.name }
}()

func featureReport(_ interface: Interface, _ id: Int, _ bytes: Int) -> [UInt8]? {
    var report = [UInt8](repeating: 0, count: max(bytes, 8))
    var length = report.count
    guard IOHIDDeviceGetReport(interface.device, kIOHIDReportTypeFeature, id, &report, &length) == kIOReturnSuccess else { return nil }
    return Array(report.prefix(length))
}

func hex(_ bytes: [UInt8]) -> String { bytes.map { String(format: "%02x", $0) }.joined() }

/// Every feature report of the display that can be read, as hex. Reports above `limit` bytes are
/// skipped (the preset interface has blobs of several kilobytes, read only for snapshots).
func hidState(limit: Int) -> [String: String] {
    var state: [String: String] = [:]
    for interface in interfaces {
        for report in interface.featureReports where report.bytes > 0 && report.bytes <= limit {
            let key = "\(interface.name)/\(String(format: "%02x", report.id))"
            state[key] = featureReport(interface, report.id, report.bytes).map(hex) ?? "unreadable"
        }
    }
    return state
}

func backlightNits() -> Double? {
    guard let monitor = interfaces.first(where: { $0.name.hasPrefix("0080:") }), let report = featureReport(monitor, 1, 7), report.count >= 5 else { return nil }
    return Double(UInt32(report[1]) | UInt32(report[2]) << 8 | UInt32(report[3]) << 16 | UInt32(report[4]) << 24) / 100
}

// MARK: - Ambient light as macOS sees it

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

/// Per light sensor, the fields of its ambient light event: [0] is lux; the others are the raw
/// channels and colour components (which is which is part of what the experiment shows).
func ambient() -> [[Double]] {
    lightServices.map { service in
        guard let event = IOHIDServiceClientCopyEvent(service, ambientLightEvent, 0, 0) else { return [] }
        return (0..<12).map { (IOHIDEventGetFloatValue(event, Int32(ambientLightEvent << 16) + Int32($0)) * 10000).rounded() / 10000 }
    }
}

// MARK: - Samples

let brightness = Brightness()

func sample(hidLimit: Int) -> [String: Any] {
    var values: [String: Any] = ["properties": brightness.all(), "hid": hidState(limit: hidLimit), "ambient": ambient()]
    if let value = slider() { values["slider"] = value }
    if let value = backlightNits() { values["nits"] = value }
    if let value = transferTable() { values["transfer"] = value }
    if let display = display, let hdr = hdrOn { values["hdr"] = hdr(display) }
    return values
}

func json(_ value: Any, pretty: Bool = false) -> String {
    let options: JSONSerialization.WritingOptions = pretty ? [.prettyPrinted, .sortedKeys] : [.sortedKeys]
    return (try? JSONSerialization.data(withJSONObject: value, options: options)).flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
}

/// Flattens a sample to "path = value" lines so two samples can be compared.
func flatten(_ value: Any, _ path: String = "") -> [String: String] {
    if let dictionary = value as? [String: Any] {
        return dictionary.reduce(into: [:]) { result, entry in result.merge(flatten(entry.value, path.isEmpty ? entry.key : "\(path).\(entry.key)")) { a, _ in a } }
    }
    return [path: value is [Any] ? json(value) : String(describing: value)]
}

func differences(_ before: [String: Any], _ after: [String: Any]) -> [String] {
    let (a, b) = (flatten(before), flatten(after))
    return Set(a.keys).union(b.keys).sorted().compactMap { key in
        a[key] == b[key] ? nil : "  \(key):\n      \(short(a[key] ?? "(absent)"))\n   -> \(short(b[key] ?? "(absent)"))"
    }
}

func short(_ text: String) -> String { text.count > 400 ? String(text.prefix(400)) + " ...(\(text.count) chars)" : text }

func describeSetup() {
    print("# display found: \(display != nil), HID interfaces: \(interfaces.map { "\($0.name)(\($0.featureReports.map { "\($0.id):\($0.bytes)" }.joined(separator: ",")))" }.joined(separator: " "))")
    print("# light sensors: \(lightServices.count), True Tone supported=\(String(describing: brightness.trueToneSupported)) available=\(String(describing: brightness.trueToneAvailable)) enabled=\(String(describing: brightness.trueToneEnabled))")
}

func say(_ text: String) {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/say")
    process.arguments = text.unicodeScalars.contains { $0.value > 0x2E80 } ? ["-v", "Tingting", text] : [text]
    try? process.run()
}

func wait(_ seconds: Double) { Thread.sleep(forTimeInterval: seconds) }

// MARK: - Commands

let arguments = CommandLine.arguments
let command = arguments.count > 1 ? arguments[1] : ""
describeSetup()

switch command {
case "snapshot":
    let label = arguments.count > 2 ? arguments[2] : "snapshot"
    let text = json(sample(hidLimit: 1 << 20), pretty: true)
    try? text.write(toFile: "tt-\(label).json", atomically: true, encoding: .utf8)
    print("written tt-\(label).json (\(text.count) characters)")

case "toggle":
    // Same light throughout; the only thing that changes is the True Tone switch.
    guard brightness.trueToneSupported == true, let original = brightness.trueToneEnabled else {
        print("True Tone is not available - is the display connected to this Mac?"); exit(2)
    }
    var samples: [(String, [String: Any])] = []
    for (label, state, settle) in [("as found", original, 2.0), ("off", false, 12.0), ("on", true, 30.0), ("off again", false, 12.0)] {
        if label != "as found" { print("True Tone \(state ? "on" : "off"): \(brightness.setTrueTone(state))") }
        wait(settle)
        let value = sample(hidLimit: 1 << 20)
        samples.append((label, value))
        try? json(value, pretty: true).write(toFile: "tt-toggle-\(label.replacingOccurrences(of: " ", with: "-")).json", atomically: true, encoding: .utf8)
    }
    print("restoring True Tone to \(original): \(brightness.setTrueTone(original))")
    for index in 1..<samples.count {
        print("\n== \(samples[index - 1].0) -> \(samples[index].0)")
        let changed = differences(samples[index - 1].1, samples[index].1)
        print(changed.isEmpty ? "  nothing readable changed" : changed.joined(separator: "\n"))
    }

case "log":
    let seconds = arguments.count > 2 ? Double(arguments[2]) ?? 600 : 600
    // The lamp steps of the experiment: seconds from the start, what to say. Each step lasts until the next.
    let guide: [(Double, String, String)] = arguments.count > 3 && arguments[3] == "guide" ? [
        (0, "dark", "实验开始。请关掉台灯，保持房间全黑。"),
        (60, "warmest-mid", "请打开台灯，色温调到最暖，亮度调到中间。"),
        (120, "warm-mid", "色温调高一档。"),
        (180, "neutral-mid", "色温再调高一档。"),
        (240, "cool-mid", "色温再调高一档。"),
        (300, "coolest-mid", "色温调到最冷。"),
        (360, "coolest-high", "保持最冷，亮度调到最高。"),
        (420, "warmest-high", "保持最高亮度，色温调到最暖。"),
        (480, "warmest-low", "保持最暖，亮度调到最低。"),
        (540, "coolest-low", "保持最低亮度，色温调到最冷。"),
        (600, "jump-warm-1", "下面测变化速度。亮度调到中间，色温直接调到最暖。"),
        (660, "jump-cool-1", "直接调到最冷。"),
        (720, "jump-warm-2", "直接调到最暖。"),
        (780, "jump-cool-2", "直接调到最冷。"),
        (840, "done", "实验结束，可以开灯了。谢谢。"),
    ] : []
    let total = guide.last.map { max(seconds, $0.0 + 10) } ?? seconds
    FileManager.default.createFile(atPath: "tt-log.jsonl", contents: nil)
    guard let file = FileHandle(forWritingAtPath: "tt-log.jsonl") else { print("cannot write tt-log.jsonl"); exit(1) }
    let start = Date()
    var step = "", nextStep = 0, lastLine = "", lastWritten = -10.0, tick = 0, slow: [String: String] = [:]
    while Date().timeIntervalSince(start) < total {
        let elapsed = Date().timeIntervalSince(start)
        while nextStep < guide.count && elapsed >= guide[nextStep].0 {
            step = guide[nextStep].1
            say(guide[nextStep].2)
            print(String(format: "%.0f s: %@", elapsed, step))
            nextStep += 1
        }
        // HID reports are read once a second (and only the small ones); everything else five times.
        if tick % 5 == 0 { slow = hidState(limit: 2048) }
        tick += 1
        var values: [String: Any] = ["step": step, "properties": brightness.all(), "hid": slow, "ambient": ambient()]
        if let value = slider() { values["slider"] = value }
        if let value = backlightNits() { values["nits"] = value }
        if let value = transferTable() { values["transfer"] = value }
        let line = json(values)
        if line != lastLine || elapsed - lastWritten >= 5 {
            values["t"] = (elapsed * 10).rounded() / 10
            file.write((json(values) + "\n").data(using: .utf8)!)
            lastLine = line
            lastWritten = elapsed
        }
        wait(0.2)
    }
    print("written tt-log.jsonl")

case "settings":
    // Same light; does the adaptation depend on the screen brightness or on HDR?
    guard let display = display, let before = slider() else { print("no display"); exit(2) }
    var wasAuto = false
    _ = autoBrightnessOn?(display, &wasAuto)
    let wasHdr = hdrOn?(display)
    // Automatic brightness is switched off first: setting the slider while it is on teaches macOS
    // a new brightness preference.
    _ = setAutoBrightness?(display, false)
    wait(2)
    var samples: [(String, [String: Any])] = []
    func take(_ label: String) {
        wait(8)
        let value = sample(hidLimit: 2048)
        samples.append((label, value))
        try? json(value, pretty: true).write(toFile: "tt-settings-\(label).json", atomically: true, encoding: .utf8)
        print("taken: \(label)")
    }
    for level in [Float(0.3), 0.6, 0.9] {
        _ = setSlider?(display, level)
        take("slider-\(level)")
    }
    _ = setSlider?(display, before)
    if let wasHdr = wasHdr, let setHdr = setHdr {
        _ = setHdr(display, !wasHdr)
        take("hdr-\(!wasHdr)")
        _ = setHdr(display, wasHdr)
        take("hdr-\(wasHdr)")
    }
    _ = setSlider?(display, before)
    _ = setAutoBrightness?(display, wasAuto)
    print("restored: slider \(before), automatic brightness \(wasAuto), HDR \(String(describing: wasHdr))")
    for index in 1..<samples.count {
        print("\n== \(samples[index - 1].0) -> \(samples[index].0)")
        let changed = differences(samples[index - 1].1, samples[index].1)
        print(changed.isEmpty ? "  nothing readable changed" : changed.joined(separator: "\n"))
    }

case "nightshift":
    // Night Shift moves the white point too and would be mixed into every reading, so it is switched
    // off for the experiment ("nightshift off") and put back afterwards ("nightshift on").
    guard let client = (NSClassFromString("CBBlueLightClient") as? NSObject.Type)?.init() else { print("no CBBlueLightClient"); exit(1) }
    var status = [UInt8](repeating: 0, count: 128)   // { BOOL active; BOOL enabled; ... }
    typealias GetStatus = @convention(c) (AnyObject, Selector, UnsafeMutableRawPointer) -> Bool
    typealias SetEnabled = @convention(c) (AnyObject, Selector, Bool) -> Bool
    let get = unsafeBitCast(client.method(for: NSSelectorFromString("getBlueLightStatus:")), to: GetStatus.self)
    let set = unsafeBitCast(client.method(for: NSSelectorFromString("setEnabled:")), to: SetEnabled.self)
    if arguments.count > 2, arguments[2] == "off" || arguments[2] == "on" {
        print("set enabled \(arguments[2] == "on"): \(set(client, NSSelectorFromString("setEnabled:"), arguments[2] == "on"))")
        wait(3)
    }
    print("status read: \(get(client, NSSelectorFromString("getBlueLightStatus:"), &status)) active=\(status[0]) enabled=\(status[1]) bytes=\(hex(Array(status.prefix(40))))")
    print("ColorRamp: \(brightness.property("ColorRamp") ?? "-")")

default:
    print("usage: mac-truetone snapshot <label> | toggle | log <seconds> [guide] | settings | nightshift [off|on]")
}
