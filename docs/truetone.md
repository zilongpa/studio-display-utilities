# True Tone: what was measured

How the rule in `crates/sdu-core/src/truetone.rs` was found. All measurements were taken on a Mac
(macOS 27) driving a Studio Display XDR, with the programs in `tools/`.

## Where True Tone happens

macOS does it; the display does not. With True Tone switched on and off at constant light
(`tools/mac-truetone.swift toggle`, record in `truetone/tt-toggle-ns-off.txt`) none of the display's
readable HID reports changed, except that macOS lowers the light sensors' chromaticity change
threshold (the last four bytes of their feature report) so that they report colour changes at all.
What changes is on the Mac's side: the colour matrix and white point target of its brightness
service.

## The rule

`tools/mac-truetone.swift log` recorded, five times a second, both sensors' readings as macOS sees
them (lux, XYZ, colour temperature) and the white point macOS was aiming for, while a lamp was
stepped through five colour temperatures and three brightness levels (`truetone/tt-log.jsonl`).
One more point comes from greenish daylight, taken later.

The steady states, with the ambient light as the sum of both sensors:

| Ambient light (x, y) | About | White macOS chose (x, y) |
|---|---|---|
| 0.4433, 0.4108 | 2940 K | 0.3413, 0.3512 |
| 0.4120, 0.3943 | 3600 K | 0.3413, 0.3512 |
| 0.3821, 0.3769 | 4200 K | 0.3390, 0.3501 |
| 0.3569, 0.3617 | 4800 K | 0.3321, 0.3454 |
| 0.3337, 0.3475 | 5350 K | 0.3227, 0.3396 |
| 0.3343, 0.3595 | greenish daylight | 0.3239, 0.3463 |

Fitted over all 13 steady states: the white moves half of the way from D65 towards the ambient
chromaticity in the CIE 1976 u'v' diagram, limited to 0.0191 from D65. Root mean square error 0.002,
largest 0.003. Using only the brighter sensor fits a little worse (0.0024 / 0.0042); a rule on colour
temperature alone fails for light that is off the black body locus, such as the daylight point.

The same white came out at 2700, 8000 and 13500 lux, at different screen brightness, and with HDR
on or off (`truetone/tt-settings.txt`). A change starts within a second and takes about five.

In the dark (`truetone/tt-dark.jsonl`, ten minutes at 0 lux) the white stays where it was, and the
sensors' colour readings are noise.

## Checking against a Mac by photograph

With the camera's white balance and exposure locked, a white screen was photographed on the Mac with
True Tone off and on (`tools/mac-photo.swift`), and on Windows with each method. White's green and
blue relative to red, in linear light, relative to True Tone off:

| | Green | Blue |
|---|---|---|
| Mac, True Tone on | 0.96 | 0.84 |
| what macOS' own matrix says for that moment | 0.99 | 0.84 |
| what the rule gives for that moment | 0.99 | 0.84 |

## The white point methods

Measured with the display's own light sensor, which sees the screen's light reflected from the room:

- The gamma ramp is applied to the signal on its way to the display. In HDR that signal is PQ
  encoded with BT.2020 primaries; a table worked out accordingly gives the same white as the colour
  effect, on white and on greys, within the sensor's resolution. Multiplying the encoded values
  directly, as one does in SDR, is about twice too strong in HDR.
- The colour effect works on encoded values in SDR and on linear light in HDR.
- The AMD driver's colour temperature is the same in SDR and HDR and close to its label, but cannot
  leave the line of plain colour temperatures.
- Only the colour effect shows in a screen capture.
