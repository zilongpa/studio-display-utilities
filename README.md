# Studio Display Utilities

Automatic brightness, the keyboard's brightness keys, HDR and True Tone for Apple Studio Displays on
Windows, made to behave the way the display does on a Mac.

Developed and tested on a Studio Display XDR (USB `05AC:1116`) with an AMD graphics card.

One small tray program, `sdu.exe`: written in Rust, no dependencies, a few hundred kilobytes.

## What it does

- **Automatic brightness** from the display's two ambient light sensors, on the curve macOS uses:
  41 nits in the dark, about 1.08 nits more per lux, up to 600 nits. Changes fade at the pace macOS
  uses (about 2 s when the room gets brighter, 5 s when it gets darker).
- **Brightness keys.** The keyboard's own brightness keys change Windows' brightness value and show
  the system's brightness bar; the program follows that value and keeps it in step with the real
  brightness, so the bar is always right. The 0 – 100 % scale is the macOS brightness slider: the
  same position gives the same brightness as on a Mac. Works in full screen games.
- **F14 and F15.** As on a Mac, they turn the brightness down and up by one of sixteen notches, for
  keyboards without brightness keys; this can be switched off in the menu. (The system's brightness bar only shows for the real keys.)
- **Keys and automatic brightness together.** A manual change is remembered as a factor on the
  curve, so brightness keeps following the room around the level you chose.
- **SDR and HDR.** In SDR the display's backlight is set (4 – 600 nits). In HDR the backlight has no
  visible effect, so Windows' "SDR content brightness" is set instead (80 – 480 nits).
- **True Tone.** The screen's white follows the colour of the ambient light, by the rule measured on
  macOS (see below). Off by default.
- **Reference modes.** Switches between the display's colour presets (P3-2000 nits, HDTV, sRGB, …).
  As on macOS only the "Apple …" presets have adjustable brightness; in HDR only the
  "Apple XDR Display …" ones can be used.
- **HDR on AMD cards.** Makes Windows offer HDR for the display, and keeps it on (see below).
- **Rotation.** Turns the picture when the screen is turned on its stand.
- **Starts at sign-in** if you want, and reconnects by itself after sleep or when the display is
  plugged in again.

Everything is in the tray icon's menu.

## Installation

1. Get `sdu.exe` from the [releases](../../releases) and run it. Only one instance runs at a time.
2. Sensors. Windows' own driver fails to start on the display's sensor interfaces, so they have to
   be given to WinUSB once. When the display is there and a sensor is not, the menu shows
   **Install sensor drivers (administrator)…**: choose it and confirm the Windows prompt.
   Without this step brightness keys, HDR and reference modes still work; automatic brightness,
   True Tone and rotation need it.

   What the installation does: it adds a driver package that contains no driver code, only a text
   file assigning Microsoft's WinUSB to interfaces 8 and 9. Windows requires such a package to be
   signed, so a certificate is created for this one use, its public half is added to the machine's
   trusted root and trusted publisher stores, and its private key is destroyed.
   `sdu.exe admin sensors-remove` (with the tray program closed) removes the package and the
   certificate again. [Zadig](https://zadig.akeo.ie/) does the same thing by hand, if you prefer.

Settings and a log are kept in `%APPDATA%\sdu\`.

## True Tone

macOS does True Tone itself; the display only supplies the sensor readings. Measured on a Mac
driving the same display:

- The ambient light is the light of both sensors added up.
- The white moves half of the way from D65 towards the ambient light's chromaticity (in the
  CIE 1976 u'v' diagram), but no further than 0.0191 from D65: under warm lamps it ends at about
  5150 K. It follows the colour, not just the colour temperature: greenish light gives a greenish white.
- It does not depend on how bright the room is, on the screen brightness or on HDR.
- In the dark the sensors report no usable colour and the white stays where it was.
- A change takes about five seconds.

Over 13 recorded steady states the rule is within 0.003 (u'v') of what macOS chose; see
[docs/truetone.md](docs/truetone.md).

Windows has no single way to change a display's white point, so there is a choice in the menu:

| Method | SDR | HDR | In screenshots | Notes |
|---|---|---|---|---|
| Gamma ramp (default) | yes | yes | no | Exact, on any graphics card. In HDR the table is worked out through the PQ curve and BT.2020 primaries. Windows resets it when the HDR brightness changes, so the white steps back to neutral while brightness moves and fades back afterwards. |
| Colour effect | yes | yes | **yes** | Exact. The compositor's full screen colour matrix; affects all screens and replaces Windows' colour filters. |
| Graphics driver (AMD) | yes | yes | no | The driver's colour temperature, in steps of 100 K. Only knows colour temperatures: a greenish or pinkish white becomes the nearest plain one. |

## HDR on AMD cards

The display describes itself with DisplayID 2.0 only. The AMD driver understands that but does not
pass it on to Windows, which then sees a generic monitor without an HDR switch. (Tools that write an
EDID override to the registry do not help; the driver does not use it.)

    sdu.exe hdr

gives the driver a standard EDID in its place and switches HDR on. The EDID is not a canned file: it
is built from the display's own description each time — model, size, colour gamut, bit depth,
refresh rate range and HDR metadata are taken from the display, the 5120 × 2880 @ 120 Hz timing is
the display's own, and 60 / 50 / 48 Hz are added by the VESA CVT-RB2 formula.

The driver forgets the EDID whenever the display is detected afresh (restart, replug, a change of
reference mode or rotation). With **Keep HDR on** ticked in the menu the program puts it back by
itself; the screen flickers for a few seconds when that happens.

`sdu.exe edid <folder>` writes the display's description and the EDID that would be used to a
folder, without changing anything. No administrator rights are needed. AMD only, through the
driver's ADL library; tested on a Radeon AI PRO R9700 with AMD Software PRO Edition.

## Limits

- Windows' own brightness slider does not appear; the brightness bar of the keys does.
- In HDR the brightness range is 80 – 480 nits (Windows' limit), and only SDR content is affected.
- The orientation sensor only tells the stand's two positions: upright, and a quarter turn clockwise.
- A full screen game or another colour tool may replace the gamma ramp; the program writes it again
  within a second.

## Commands

    sdu.exe                              the tray program
    sdu.exe hdr | hdr off                HDR on an AMD card: make it available and switch it on / off
    sdu.exe edid <folder>                write the display's description and the generated EDID
    sdu.exe sensor [seconds]             print the light sensors' readings
    sdu.exe orientation [seconds]        print the orientation sensor's reports
    sdu.exe presets [number]             list the reference modes / switch to one
    sdu.exe backlight <nits>             set the backlight
    sdu.exe white <method> <kelvin|x,y> [seconds]
                                         show a white point with one of the methods, then undo it
    sdu.exe admin sensors | sensors-remove
                                         install / remove the WinUSB binding of the sensors
    sdu.exe icon <file.bmp> [size]       write the icon to a file

## Building

    cargo build --release

Needs Rust (MSVC toolchain) and the Windows SDK. GitHub Actions builds and tests every push; a
`v*` tag publishes a release.

## Layout

A Cargo workspace of three crates. Everything that is plain calculation is in `sdu-core`, which
builds anywhere; a port to another system only has to supply the device access.

| Crate | Contents |
|---|---|
| `sdu-core` | The macOS brightness scale, the automatic brightness curve, the True Tone rule and the colour maths, fades, EDID generation, the icon |
| `sdu-windows` | The display's USB interfaces (backlight, sensors, reference modes), Windows' brightness value, HDR and rotation, the white point methods, the AMD driver, elevation |
| `sdu` | The tray program and the command line |

## How the numbers were found

The brightness scale, the automatic brightness curve, the fade times and the True Tone rule were
measured on a Mac driving the same display. The data is in `docs/`; the programs that recorded it
are in `tools/` (the `*.swift` files run on the Mac).

## Hardware notes

- **Backlight**: USB interface 7, HID feature report 1: `[01] [brightness: u32 LE, 0.01 nit] [u16: 0]`,
  400 – 60000.
- **Ambient light**: interface 8, two sensors (report ids 1 and 2). Input report, 18 bytes:
  `[id] [state] [illuminance: u32, millilux] [colour temperature: u32, K] [x: u32] [y: u32]`
  (x, y in units of 1e-8). The last four bytes of the 41 byte feature report set how far the
  chromaticity must move before the sensor reports again.
- **Orientation**: interface 9, input report 1: three 9 bit angles in degrees; the second one is the
  turn of the screen.
- **Reference modes**: interface 6, vendor usage page 0xFF20: usage 3 is the active preset, usage 4
  a write-only cursor, usages 6 and 8 the validity and name of the preset under the cursor.
- **SDR content brightness**: read with `DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL`; written with
  the request Windows' Settings uses (type `0xFFFFFFEE`), which is not in the public headers.

## Thanks

The reference mode protocol was worked out by
[Studio Brightness++](https://github.com/LitteRabbit-37/Studio-Brightness-PlusPlus).

## Licence

MIT
