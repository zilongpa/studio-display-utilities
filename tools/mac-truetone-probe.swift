// Lists what macOS exposes about True Tone through CoreBrightness (private framework), without
// changing anything: which client classes exist, their methods, and what they currently report.
// Used to decide what a True Tone experiment can observe.

import Foundation
import ObjectiveC

setvbuf(stdout, nil, _IONBF, 0)
guard dlopen("/System/Library/PrivateFrameworks/CoreBrightness.framework/CoreBrightness", RTLD_NOW) != nil else {
    print("CoreBrightness could not be loaded"); exit(1)
}

func methods(of cls: AnyClass) -> [String] {
    var count: UInt32 = 0
    guard let list = class_copyMethodList(cls, &count) else { return [] }
    defer { free(list) }
    return (0..<Int(count)).map { NSStringFromSelector(method_getName(list[$0])) }.sorted()
}

for name in ["CBTrueToneClient", "CBAdaptationClient", "CBBlueLightClient", "BrightnessSystemClient", "CBClient", "CBColorFilter", "CBKeyboardBacklightClient"] {
    guard let cls = NSClassFromString(name) else { print("\(name): not present"); continue }
    print("\(name): \(methods(of: cls).joined(separator: ", "))")
}

// Read-only state of True Tone.
if let cls = NSClassFromString("CBTrueToneClient") as? NSObject.Type {
    let client = cls.init()
    for selector in ["supported", "available", "enabled"] where client.responds(to: NSSelectorFromString(selector)) {
        typealias Getter = @convention(c) (AnyObject, Selector) -> Bool
        let imp = client.method(for: NSSelectorFromString(selector))
        print("CBTrueToneClient.\(selector) = \(unsafeBitCast(imp, to: Getter.self)(client, NSSelectorFromString(selector)))")
    }
}

// Properties the brightness daemon hands out by name: try the ones that look related.
if let cls = NSClassFromString("BrightnessSystemClient") as? NSObject.Type {
    let client = cls.init()
    let copy = NSSelectorFromString("copyPropertyForKey:")
    if client.responds(to: copy) {
        let keys = ["ColorAdaptationEnabled", "ColorAdaptationStrength", "CBColorAdaptationEnabled", "CBColorAdaptationStrength", "ColorAdaptationAvailable",
                    "HarmonyEnabled", "HarmonyStrength", "WhitePoint", "CBWhitePoint", "AdaptationStrength", "ColorRamp", "CBColorRamp", "TrustedLux",
                    "AmbientColor", "CBAmbientColor", "ALSColor", "CCT", "AmbientCCT", "DisplayBrightness", "DisplayBrightnessAuto", "CBDisplayPresets",
                    "BlueLightStatus", "CBBlueLightReductionCCTTargetRaw", "AmbientAdaptiveDimming", "ColorAdaptationModeMapping", "AmmoliteEnabled",
                    "SupportedColorFX", "ColorFadesEnabled", "CBSoftWakeActive", "AggregatedLux", "TrustedFrontLux", "DisplayNits", "CBDisplayNits"]
        for key in keys {
            if let value = client.perform(copy, with: key)?.takeRetainedValue() {
                print("property \(key) = \(String(describing: value).prefix(300))")
            }
        }
    }
}
