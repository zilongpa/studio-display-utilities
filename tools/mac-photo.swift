// Shows a labelled full-screen white window on the Mac with True Tone off and then on, so both can be
// photographed with a locked white balance and compared with the same photos taken on Windows.
// Prints what macOS reports at each step (ambient light, its white point target).
// Night Shift is switched off for the duration; it and True Tone are put back afterwards.
//
//   swiftc -O mac-photo.swift -o photo && ./photo [seconds per picture]

import AppKit
import Foundation

setvbuf(stdout, nil, _IONBF, 0)
dlopen("/System/Library/PrivateFrameworks/CoreBrightness.framework/CoreBrightness", RTLD_NOW)

let system = (NSClassFromString("BrightnessSystemClient") as? NSObject.Type)?.init()
let trueTone = (NSClassFromString("CBTrueToneClient") as? NSObject.Type)?.init()
let nightShift = (NSClassFromString("CBBlueLightClient") as? NSObject.Type)?.init()

func property(_ key: String) -> String {
    let selector = NSSelectorFromString("copyPropertyForKey:")
    guard let system = system, system.responds(to: selector), let value = system.perform(selector, with: key)?.takeRetainedValue() else { return "-" }
    return String(describing: value).replacingOccurrences(of: "\n", with: " ").replacingOccurrences(of: "    ", with: "")
}

typealias Getter = @convention(c) (AnyObject, Selector) -> Bool
typealias Setter = @convention(c) (AnyObject, Selector, Bool) -> Bool
typealias GetStatus = @convention(c) (AnyObject, Selector, UnsafeMutableRawPointer) -> Bool

func trueToneOn() -> Bool {
    guard let client = trueTone else { return false }
    let selector = NSSelectorFromString("enabled")
    return unsafeBitCast(client.method(for: selector), to: Getter.self)(client, selector)
}

func set(_ client: NSObject?, _ on: Bool) {
    guard let client = client else { return }
    let selector = NSSelectorFromString("setEnabled:")
    _ = unsafeBitCast(client.method(for: selector), to: Setter.self)(client, selector, on)
}

func nightShiftOn() -> Bool {
    guard let client = nightShift else { return false }
    var status = [UInt8](repeating: 0, count: 128)   // { BOOL active; BOOL enabled; ... }
    let selector = NSSelectorFromString("getBlueLightStatus:")
    _ = unsafeBitCast(client.method(for: selector), to: GetStatus.self)(client, selector, &status)
    return status[1] != 0
}

@_silgen_name("IOHIDEventSystemClientCreate") func IOHIDEventSystemClientCreate(_ allocator: CFAllocator?) -> AnyObject?
@_silgen_name("IOHIDEventSystemClientCopyServices") func IOHIDEventSystemClientCopyServices(_ client: AnyObject) -> CFArray?
@_silgen_name("IOHIDServiceClientCopyEvent") func IOHIDServiceClientCopyEvent(_ service: AnyObject, _ type: Int64, _ options: Int32, _ timestamp: Int64) -> AnyObject?
@_silgen_name("IOHIDServiceClientCopyProperty") func IOHIDServiceClientCopyProperty(_ service: AnyObject, _ key: CFString) -> AnyObject?
@_silgen_name("IOHIDEventGetFloatValue") func IOHIDEventGetFloatValue(_ event: AnyObject, _ field: Int32) -> Double
let lightEvent: Int64 = 12
let eventClient = IOHIDEventSystemClientCreate(kCFAllocatorDefault)
let lightServices = ((eventClient.flatMap { IOHIDEventSystemClientCopyServices($0) } as? [AnyObject]) ?? []).filter {
    (IOHIDServiceClientCopyProperty($0, "VendorID" as CFString) as? Int) == 0x05ac && IOHIDServiceClientCopyEvent($0, lightEvent, 0, 0) != nil
}

/// Per light sensor: lux (field 0), X Y Z (7 - 9), colour temperature (10).
func ambient() -> String {
    lightServices.map { service -> String in
        guard let event = IOHIDServiceClientCopyEvent(service, lightEvent, 0, 0) else { return "-" }
        let field = { (index: Int32) in IOHIDEventGetFloatValue(event, Int32(lightEvent << 16) + index) }
        return String(format: "lux %.0f  XYZ %.1f %.1f %.1f  CCT %.0f", field(0), field(7), field(8), field(9), field(10))
    }.joined(separator: " | ")
}

func report(_ label: String) {
    print("\(label): TrueTone=\(trueToneOn()) NightShift=\(nightShiftOn())")
    print("  ambient: \(ambient())")
    print("  ColorRamp: \(property("ColorRamp"))")
    print("  strength: \(property("ColorAdaptationStrength"))")
}

let seconds = CommandLine.arguments.count > 1 ? Double(CommandLine.arguments[1]) ?? 25 : 25

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
var labels: [NSTextField] = []
var windows: [NSWindow] = []
for screen in NSScreen.screens {
    let window = NSWindow(contentRect: screen.frame, styleMask: .borderless, backing: .buffered, defer: false)
    window.level = .screenSaver
    window.backgroundColor = .white
    window.isOpaque = true
    let label = NSTextField(labelWithString: "")
    label.font = NSFont.systemFont(ofSize: 44)
    label.textColor = .black
    label.alignment = .center
    label.frame = NSRect(x: 0, y: screen.frame.height / 2 - 120, width: screen.frame.width, height: 240)
    label.maximumNumberOfLines = 4
    window.contentView?.addSubview(label)
    window.setFrame(screen.frame, display: true)
    window.orderFrontRegardless()
    labels.append(label)
    windows.append(window)
}
print("screens: \(NSScreen.screens.map { "\($0.localizedName) \(Int($0.frame.width))x\(Int($0.frame.height))" })")

func show(_ text: String) {
    DispatchQueue.main.sync { labels.forEach { $0.stringValue = text } }
}

/// Shows `text` with a countdown; calls `midway` once, half way through.
func hold(_ text: String, _ duration: Double, midway: () -> Void = {}) {
    var reported = false
    var left = duration
    while left > 0 {
        show("\(text)\n\n剩余 \(Int(left.rounded(.up))) 秒")
        Thread.sleep(forTimeInterval: 1)
        left -= 1
        if !reported && left <= duration / 2 { reported = true; midway() }
    }
}

Thread.detachNewThread {
    let trueToneBefore = trueToneOn()
    let nightShiftBefore = nightShiftOn()
    print("before: TrueTone=\(trueToneBefore) NightShift=\(nightShiftBefore)")
    if nightShiftBefore { set(nightShift, false) }

    set(trueTone, false)
    hold("Mac 准备中（True Tone 正在关闭）", 10)
    hold("1    Mac    True Tone 关", seconds) { report("picture 1") }

    set(trueTone, true)
    hold("Mac 准备中（True Tone 正在打开）", 12)
    hold("2    Mac    True Tone 开", seconds) { report("picture 2") }

    set(trueTone, trueToneBefore)
    if nightShiftBefore { set(nightShift, true) }
    hold("拍完了。可以把线换回 Windows", 8)
    print("restored: TrueTone=\(trueToneOn()) NightShift=\(nightShiftOn())")
    exit(0)
}
app.run()
