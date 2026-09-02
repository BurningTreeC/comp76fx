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
the gain element rather than before it. That is not a detail. With `k` as the
sidechain's gain, the loop settles at

```
g = -k / (1 + k) * (input - threshold)
```

so the slope is `1 / (1 + k)` and the ratio is simply `1 + k`. The 4:1 button
is `k = 3`; the 20:1 button is `k = 19`. The soft knee everyone describes is
not dialled in anywhere, it is what a feedback loop does.

* **The gain element is a FET** used as a voltage controlled resistor, and it
  distorts the audio passing through it increasingly as it is pulled down.
  That is most of why the unit sounds the way it does when it is working hard.
* **The recovery runs two stages together**, so it is program dependent rather
  than a fixed curve.
* **The sidechain saturates smoothly** rather than clipping, because an
  amplifier running out of rail bends over. Without that, every attack setting
  collapses to the same time, and a hard limit would turn into an infinite
  ratio the moment it was reached.
* **All-button mode** puts a lag in the control path before the gain
  collapses, which is the "reverse look-ahead" that lets the front of every
  transient through, and shifts the timing and bias with it.

Measured by the tests in `core/tests/compression.rs`:

| | |
| --- | --- |
| 4:1 button | 3.98:1 |
| 8:1 button | 7.83:1 |
| 12:1 button | 11.66:1 |
| 20:1 button | 19.27:1 |
| all four in | 14.63:1 |
| no buttons in | exactly 1:1, colour with no gain reduction |
| attack | 0.010 ms fastest to 0.250 ms slowest, closed loop |
| release | 91 ms fastest to 1959 ms slowest |
| distortion | Rev A −35.5 dB, Rev D −39.0 dB, Rev F −57.2 dB |

The marked attack and release times, 20 µs to 800 µs and 50 ms to 1.1 s, are
the detector's own time constants. The figures above are the closed loop
behaviour, which is necessarily faster: the sidechain asks for far more
reduction than the loop settles at, so the envelope passes 63 % of its
settling point well before one time constant has elapsed.

## Controls

The panel is the hardware's. **Input** drives the signal against a fixed
operating point, which is how the unit is threshold-less; **output** is
make-up. Attack and release are marked slowest to fastest, which is backwards
from most compressors and is how the originals were engraved.

The four **ratio** switches are mechanically interlocked, so clicking one
releases the others. **Hold shift or ctrl to latch**, which is how you get all
four in at once without having to be quick with your fingers.

The **meter** switch selects gain reduction, output level referenced to +4 or
+8, or off.

The strip above the panel is not on the hardware. It carries the preset drop
down, a save button and the settings button, which holds the window scale
(50 % to 200 %), the oversampling quality and the dry blend.

Built-in presets include **All Buttons In**: all four switches in, both dials
wide open, driven hard.

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
| `core/src/dsp/detector.rs` | the feedback sidechain and its timing |
| `core/src/dsp/fet.rs` | the gain element and its distortion |
| `core/src/dsp/amp.rs` | the Class A and Class AB output stages |
| `core/src/editor/` | the front panel |
| `core/src/presets.rs` | built-in and saved presets |
| `rev_a`, `rev_d`, `rev_f` | one identity each; the circuit differences live in `Revision` |
| `core/tests/compression.rs` | the measurements above |
