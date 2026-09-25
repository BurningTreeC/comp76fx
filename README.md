# Comp76Fx

Three circuit models of the classic 1966 FET limiting amplifier, one per
revision, built with [NIH-plug](https://github.com/robbert-vdh/nih-plug). Each
builds as CLAP and VST3.

| Plugin | Circuit | Character |
| --- | --- | --- |
| **Comp76Fx Rev A** | no low noise circuit, Class A output, painted meter surround | the most aggressive and the noisiest, and its ratios undershoot their markings |
| **Comp76Fx Rev D** | low noise circuitry, Class A output | the one most people mean; the reference reissue is patterned on the D and E |
| **Comp76Fx Rev F** | low noise, push-pull Class AB output | cleaner and tighter, with the distortion turning symmetrical |

They are not affiliated with, endorsed by, or connected to Universal Audio or
any other manufacturer. Model numbers are used only to say which circuit is
modelled.

![the panel](doc/panel.png)

## What it models

The unit is a **feedback** compressor: the sidechain samples the signal after
the gain element rather than before it. It takes it from the preamplifier,
straight after the gain element and ahead of the output control and the line
amplifier, so the output stage's colour is outside the loop. That is not a
detail. With `k` as the sidechain's gain, the loop settles at

```
g = -k / (1 + k) * (input - threshold)
```

so the slope is `1 / (1 + k)` and the ratio is simply `1 + k`. The 4:1 button
is `k = 3`; the 20:1 button is `k = 19`. What the feedback buys is ratios that
land on their markings. It does not soften the knee on its own: the knee is in
the threshold circuit.

* **Each ratio has its own threshold.** The buttons switch a DC divider that
  biases the rectifier diodes along with the signal divider that feeds them,
  and the manual says "selecting higher ratios also raises the threshold
  level": −24, −25 and −26 dB for 20:1, 12:1 and 8:1, and for 4:1 "the
  lowest threshold is −30 dB", where its soft knee begins.
* **The knee is softest at 4:1.** A diode turns on over a span of voltage, not
  at a point, and that span covers the most decibels where the signal at the
  diodes is smallest. The 4:1 passes the smallest share of the smallest
  signal, about a tenth of the 20:1's, so its knee is about ten times as
  wide: the "soft knee in the threshold circuit for this ratio" that the
  manual measures its ratio test around. The manual gives no width; the size
  of all four is bounded by that test.

The loop is solved within each sample rather than run a sample behind itself.
Running it behind is a correction the size of the open loop gain, and at the
fastest attack that overshot the settling point on every transient, by as
much as 20 dB without oversampling, and held the excess for the whole release.

* **The gain element is a FET** used as a voltage controlled resistor, and it
  distorts the audio passing through it increasingly as it is pulled down.
  That is most of why the unit sounds the way it does when it is working hard.
* **The recovery runs two stages together**, so it is program dependent rather
  than a fixed curve.
* **The sidechain runs out of rail** rather than clipping, but only where a
  real one would. At equilibrium the demand and the gain reduction are the
  same number, so a curve that bends from the origin bends the static ratio
  with it and every button reads low. It stays linear across the range the
  unit works in and turns over only near the rail.
* **All-button mode is modelled on the switch bank as the schematic draws
  it.** Each button connects a tap on two resistor ladders, one feeding the
  sidechain and one in the gate's bias network, and pressing several joins
  their taps, which shorts everything between the lowest and highest pressed.
  So only the outer buttons matter (4 + 20 is all four, as Universal Audio
  notes), and all four:
  * pass the sidechain 0.456 of the signal against the 20:1's 0.802, which
    sets a threshold and ratio between the 8:1's and the 12:1's;
  * pull the gate's resting bias from −2.0 V to −3.2 V (a circuit simulation
    of the mode posted to GroupDIY), so the sidechain has to charge through a
    dead zone before the FET opens: the attack lags, the release reaches no
    reduction far sooner, the reduction is lighter than a plain ratio's at
    the same drive, and the gain reduction meter, which reads that bias,
    rests pinned past zero;
  * throw off the gate's distortion-cancelling feedback, so the gain element
    bends the signal further.

  It measures 15:1 at every drive level, the middle of the manual's
  "somewhere between 12:1 and 20:1". Two figures are not documented and are
  assumptions: how many decibels of control the 1.2 V bias drop is worth
  (taken as 30 dB, the drop's share of the gate's roughly 2 V working range),
  and how much it raises the loop gain, which is fitted to that range.

Measured by `cargo run --release -p comp76fx_core --example bench -- --spec`
and the tests in `core/tests/compression.rs`:

| | |
| --- | --- |
| 4:1 button | 4.04:1 |
| 8:1 button | 8.07:1 |
| 12:1 button | 12.12:1 |
| 20:1 button | 20.26:1 |
| all four in | 15.1:1 driven, 15.4:1 gently; 4 + 20 identical |
| threshold | 20:1 at −24.0 dBFS; 12:1, 8:1 and 4:1 at −1.0, −2.0 and −3.0 dB from it |
| knee | reduction already at threshold: 0.16, 0.27, 0.41 and 0.74 dB from 20:1 to 4:1 |
| the manual's ratio test | 20:1, 12:1 and 8:1 from 1 dB of limiting, 4:1 from 3 dB: all within 12 % against its 20 % |
| no buttons in | exactly 1:1, colour with no gain reduction |
| attack | 19.5 µs to 794 µs against a marked 20 µs to 800 µs |
| release | 50.0 ms to 1100 ms against a marked 50 ms to 1.1 s |
| distortion, idle at −18 dBFS | Rev A 0.48 %, Rev D 0.33 %, Rev F 0.05 % |
| frequency response | within 0.53 dB across 20 Hz to 20 kHz |
| signal to noise | Rev A 91 dB, Rev D 101 dB, Rev F 103 dB, at every oversampling setting |
| latency | 74 samples at every oversampling setting, dry blend included |

`core/tests/calibration.rs` holds the published figures to those tolerances,
including the response at every sample rate and oversampling setting, the ends
of the dials, and the meter's needle against the marks printed on its own
face. `core/tests/latency.rs` holds the reported latency to the real one and
the dry blend in line with the wet signal. `core/tests/threshold.rs` runs the
manual's own ratio test and holds each button's threshold and knee, and
all-button mode's switch bank, timing and meter.

One of those is worth reading twice. The ratios sit about a percent high
because the unit's own distortion takes energy out of the fundamental the
measurement reads; the loop itself, linearised, lands a percent low, and the
two nearly cancel.

## Controls

The panel is the hardware's. **Input** drives the signal against a fixed
operating point, which is how the unit is threshold-less; **output** is
make-up. Attack and release are engraved 1 to 7, slowest to fastest, which is
backwards from most compressors and is how the originals were engraved; 1 is
800 µs and 1.1 s, 7 is 20 µs and 50 ms.

The four **ratio** switches are mechanically interlocked, so clicking one
releases the others. **Hold shift or ctrl to latch**, which is how you get all
four in at once without having to be quick with your fingers.

The **meter** switch selects gain reduction, output level referenced to +4 or
+8, or off. The output positions read the average level, referred to a sine,
so a tone peaking at −18 dBFS sits on 0 VU at +4.

The strip above the panel is not on the hardware. It carries the preset drop
down, a save button and the settings button, which holds the window scale
(50 % to 200 %), the oversampling quality and the dry blend. The plugin reports
the same latency at every oversampling setting, and the dry blend and the
power switch are delayed to match, so neither moves the track against the rest
of the session.

Saved presets are one JSON file each, under a folder of the revision's own so
the three do not share: `~/.config/comp76fx-rev-a/presets` on Linux and macOS
(or under `$XDG_CONFIG_HOME`), and `%APPDATA%\Comp76Fx Rev A\Presets` on
Windows. Each carries a cross to delete it that asks before removing the file.
Saving under the name of one of your own, in any capitalisation, replaces it. A built-in preset has no file, so it cannot be deleted, and
saving under its name writes a preset of your own beside it rather than
replacing it in the list; replacing it would put it out of reach for good.

Built-in presets include **All Buttons In**: all four switches in with attack
and release fully fast, the documented setting for drum rooms and
"in your face" vocals, driven so a −18 dBFS signal takes about 10 dB of
reduction, with the make-up set so it comes back at unity, as every built-in
preset but the parallel one does.

## Building

```sh
./install.sh
```

Builds all three and installs the CLAP and VST3 of each into
`~/.clap/BurningTreeC` and `~/.vst3/BurningTreeC`. Pass `--no-build` to install
what is already built, or set `CLAP_PATH` and `VST3_PATH` to install elsewhere.

To build without installing:

```sh
cargo xtask bundle -p comp76fx_rev_a -p comp76fx_rev_d -p comp76fx_rev_f --release
```

To try one without a host:

```sh
cargo run --release -p comp76fx_rev_d --features standalone -- --backend auto
```

## Licensing

Under the **GNU General Public License version 3 or later**, whose text is in
[`LICENSE`](LICENSE). NIH-plug itself is ISC licensed, but `nih_export_vst3!()`
links the GPLv3 [vst3-sys](https://github.com/RustAudio/vst3-sys) bindings, so
any VST3 built with it has to be able to comply with the GPL.

[`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md) reproduces the licences and
copyright notices of every crate the plugins link. Noto Sans is compiled in for
the panel lettering and is under the SIL Open Font License 1.1. Regenerate the
file after changing dependencies:

```sh
python3 tools/third-party-notices.py
```

## Layout

| Path | |
| --- | --- |
| `core/src/dsp/detector.rs` | the feedback sidechain, its timing and the loop solver |
| `core/src/dsp/revisions.rs` | the circuit values of the three revisions |
| `core/src/dsp/fet.rs` | the gain element and its distortion |
| `core/src/dsp/amp.rs` | the Class A and Class AB output stages |
| `core/src/editor/` | the front panel |
| `core/src/presets.rs` | built-in and saved presets |
| `rev_a`, `rev_d`, `rev_f` | one identity each: names and plugin ids |
| `core/tests/compression.rs` | the measurements above |
| `core/tests/latency.rs` | reported latency and the dry blend's alignment |
| `core/tests/threshold.rs` | the threshold circuit against the manual |
| `vendor/baseview` | the GUI window backend, patched for Windows; see its `PATCHES.md` |
