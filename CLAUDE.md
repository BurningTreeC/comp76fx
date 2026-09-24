# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Read `AGENTS.md` and follow its rules. They cover the environment and which tools to prefer, and they apply to all work in this repository:

@AGENTS.md

## What this is

Comp76Fx: three CLAP/VST3 plugins, each modelling a different revision (A, D, F) of the 1176-style FET limiting amplifier. They are built with NIH-plug and a vizia GUI. Licensed GPL-3.0-or-later, because VST3 export links the GPL vst3-sys.

## Commands

```sh
# Build all three plugins into target/bundled/ (the xtask alias always builds release)
cargo xtask bundle -p comp76fx_rev_a -p comp76fx_rev_d -p comp76fx_rev_f --release

# Build and install into ~/.clap/BurningTreeC and ~/.vst3/BurningTreeC
./install.sh               # --no-build installs what is already bundled; CLAP_PATH / VST3_PATH override

# Run one revision without a host
cargo run --release -p comp76fx_rev_d --features standalone -- --backend auto

# Tests. Use --release: the DSP tests process seconds of audio at up to 768 kHz.
cargo nextest run --release --workspace
cargo nextest run --release -p comp76fx_core -E 'test(the_ratio_buttons_do_what_they_say)' --no-capture
cargo test --release -p comp76fx_core --test compression -- --nocapture   # one test file; tests print their measurements

# Lint exactly as CI does
cargo clippy --release --workspace --all-targets -- -D warnings

# Bench: prints ratio, attack, release, knee and THD against the published spec
cargo run --release -p comp76fx_core --example bench

# Regenerate after any dependency change. CI fails if the output differs from the committed file.
python3 tools/third-party-notices.py

# Tests for the vendored window backend
cargo test --manifest-path vendor/baseview/Cargo.toml --lib

# Windows: cross-check, and run unit tests under Wine (the x86_64-pc-windows-gnu target and wine are installed)
cargo clippy --release --target x86_64-pc-windows-gnu -p comp76fx_core -p comp76fx_rev_d -- -D warnings
cargo test --release --target x86_64-pc-windows-gnu -p comp76fx_core --lib --no-run   # then: wine <printed .exe> --test-threads=1
```

The workspace is `cargo fmt` clean, and CI runs `cargo fmt --all --check` before the tests. The code under `vendor/` is not formatted.

## Architecture

**Workspace layout.**
- `core/` holds everything: DSP, GUI, the plugin body and presets.
- `rev_a/`, `rev_d/`, `rev_f/` each contain only one `export_revision!` invocation: the plugin's name, its ids and which revision it uses. The macro generates the `Plugin`/`ClapPlugin`/`Vst3Plugin` impls. The circuit values are in `core/src/dsp/revisions.rs` (`REV_A`, `REV_D`, `REV_F`), so the tests and the bench measure exactly what ships; use `REV_X.without_noise()` for measurements. Circuit differences between revisions must be expressed as fields of `dsp::Revision`, never as per-revision code. Do not change the CLAP/VST3 ids, since hosts use them to identify saved sessions.
- `xtask/` wraps `nih_plug_xtask`.
- `installer/` is the `install.exe` shipped in the Windows archive. It has no dependencies on purpose, so nothing new needs adding to the licence notices.
- `vendor/baseview/` is a patched copy of the GUI window backend, fixing Windows mouse-capture, resize and text-entry problems. It is kept identical to the copy in the sibling `../gainstagefx` repo, apart from the project name in `PATCHES.md`, which documents every change. It is excluded from the workspace and substituted through `[patch]` in the root `Cargo.toml`. `core` also depends on it directly, only to call `baseview::set_text_input`. After copying files into it, `touch` them: cargo spots changes to path dependencies by file modification time, and `rsync -a`/`cp -p` keep the old times, which leaves a stale build in use.
- NIH-plug is pinned to one git rev in `Cargo.toml` and `xtask/Cargo.toml`. Keep the two in step.

**Signal path** (`core/src/dsp/`). `Channel::process` runs one sample through the oversampler closure in this order:
1. `Fet::process`, applying the reduction the detector asked for last.
2. `Detector::process_in_loop` is fed the FET output (the hardware's preamp output) through a 5 Hz DC-blocking coupling (`SIDECHAIN_COUPLING_HZ`), and returns the next reduction. The output stage is outside the loop, as on the hardware; `the_output_stage_is_outside_the_loop` checks it.
3. `Amplifier::process`: coupling high-pass, Class A or AB shaper, bandwidth low-pass.
4. Revision noise is added, scaled by √factor so the in-band floor is the same at every oversampling setting.

After the oversampler, a `Delay` pads the output to the constant `dsp::LATENCY` (74 samples, the 8x figure), so the latency is the same at every oversampling setting. Input and output gains are computed in `apply_controls`, not per sample. With no ratio button in, the gain element and the detector are bypassed, and the detector is reset.

The detector works in dB: demand = `limit(k · knee(level − threshold))`, then an attack follower and a two-stage release. The static ratio is `1 + k`. `process_in_loop` finds each sample's demand together with the reduction that demand causes, using a safeguarded Newton solve of `d = demand(unreduced − landing(d))`. Computing the demand one sample behind instead made fast attacks overshoot by up to 20 dB. `Detector::process` is the open-loop network alone; the calibration tests use it to check the times marked on the panel. Pressed buttons add their `k` values (parallel conductances). Each button has its own threshold (`THRESHOLD_OFFSETS_DB`, from the 1176LN manual: 20:1 at `THRESHOLD_DB`, each lower ratio 1 dB below) and a diode knee (`diode_knee_db`). The knee is 0.5 dB wide at 20:1 and scales with the inverse of the signal at the rectifier diodes (∝ k·threshold), so it is about 4.5 dB at 4:1. A combination's threshold is the k-weighted mean of the pressed buttons'. How many buttons are pressed also sets the bias shift, a further threshold drop and extra knee width. All four pressed also rescales attack and release. The tuning constants carry comments explaining their calibration. `RELEASE_COMPENSATION` must be re-solved whenever `SLOW_STAGE_SHARE` or `SLOW_STAGE_RATIO` changes; `release_compensation_is_solved` checks it.

**Plugin body** (`core/src/plugin.rs`).
- Controls are read from the parameter smoothers once per 32-sample block (`CONTROL_BLOCK`). Sample-accurate automation is off.
- The attack and release parameters hold the engraved 1–7 dial values, where 7 is fastest. `params::dial_position` maps them onto the 0–1 positions in `dsp::Controls` (1 → 0.0, 7 → 1.0).
- Each host channel is a `Strip`: a `Channel`, plus the dry signal delayed by `LATENCY` for the mix. With the power off, a `Strip` outputs the delayed dry signal alone, so the timing never shifts.
- `meters::Meters` passes meter data from the audio thread to the editor. The audio thread publishes each block: the deepest reduction (a `fetch_max`), and the output energy with its sample count, packed into one `AtomicU64`. The editor collects everything published since its last frame with `take()`, which returns `None` when no block has run since.

**Oversampler** (`dsp/oversample.rs`). A cascade of Kaiser halfband 2x stages. All stages are built up front and the factor selects how many are active, so changing the setting never allocates on the audio thread.

**Editor** (`core/src/editor/`).
- The window has a fixed logical size (`PANEL_W` × `WINDOW_H`), and every view is placed absolutely (`PositionType::SelfDirected`) from constants in `editor::layout` and `style.rs`.
- `settings.rs` holds the header strip, the settings overlay, the preset menu and the dialogs. They are driven by the `UiState` model and `UiEvent`.
- The custom widgets in `widgets.rs` are built on `ParamWidgetBase`. Any change the GUI makes to a parameter, including loading a preset, must go through a begin/set/end gesture so the host records it.
- Text labels sit on top of the controls they annotate. They must be `.hoverable(false)` (see `label_box`), or they steal the clicks.
- Images are PNGs embedded with `include_bytes!`. A `Sprite` is bound to its image bytes when it is created, and each widget owns its own, because an image id belongs to one canvas. A widget that shows two images needs two sprites (see `sprites::Cap`). The knob images are 48-frame vertical filmstrips in `assets/gen/`.
- The editor runs with `ViziaTheming::None`, and the only styling taken from a stylesheet is `style::STYLESHEET`: the text caret and the selection colour. Don't set `caret_color` inline: vizia blinks the caret by toggling a `caret` class, and an inline colour stops it blinking. Keep the sheet well-formed, because a single syntax error makes vizia silently drop the whole sheet; `the_stylesheet_is_well_formed` tests it.
- `UiState::event` calls `baseview::set_text_input(dialog == Dialog::Name)` after every event, so on Windows the save dialog's name box receives the keyboard.
- The VU needle is a spring and damper, advanced in fixed 1/240 s steps of elapsed time, so its speed doesn't depend on the frame rate.
- Window scaling: `apply_scale` must write the persisted `ViziaState` scale *before* calling `request_resize`. `core/tests/scaling.rs` pins this behaviour.

**Presets** (`core/src/presets.rs`).
- Built-in presets are written as dial values and converted against the live parameters.
- User presets are one JSON file each, in `%APPDATA%\Comp76Fx Rev X\Presets` on Windows (falling back to `%USERPROFILE%\AppData\Roaming`), and `$XDG_CONFIG_HOME` (or `~/.config`) under `<revision slug>/presets/` elsewhere. The platform choice uses `cfg!`, not `#[cfg]`, so the Windows rule is compiled and tested on Linux.
- Names are compared ignoring case (`same_name`). Saving replaces the file that preset came from, and `read_saved` is the one place that matches files to presets. A name whose cleaned-up file name is already taken gets a number added. Windows device names (`CON`, `AUX` …) get a `_` appended.
- The `os` (oversampling) and `power` parameters are never stored in a preset.
- The name of the loaded preset is saved with the host session (`#[persist = "preset"]`).

**Tests.**
- `core/tests/compression.rs`: bench-style measurements of ratio, timing, THD and the differences between revisions.
- `core/tests/calibration.rs`: detector timing, dial ends, noise floor against oversampling, VU needle geometry, frequency response at every rate and factor.
- `core/tests/latency.rs`: the reported latency equals the real one at every factor; the dry blend doesn't comb; power off keeps the timing.
- `core/tests/threshold.rs`: the 1176LN manual's own ratio test (§13 of its calibration procedure), each button's threshold and knee, and the sidechain tap.
- `the_built_in_presets_come_back_at_unity`: every built-in preset except Parallel Smash returns a −18 dBFS tone at unity. Anything that changes how hard a ratio works has to re-trim their output values.
- `core/tests/scaling.rs`: window-size persistence.
- `cargo run --release -p comp76fx_core --example bench -- --spec` checks against the published spec and prints the figures quoted in the README's measurement table. Re-run it and update the table when the DSP changes.

## Conventions

- Comments explain the physical or causal reason for a choice, often including the mistake it replaced. Keep that style and the British spelling (colour, normalised).
- The version lives in the root `Cargo.toml` under `[workspace.package]` and appears on the panel through `CARGO_PKG_VERSION`.
