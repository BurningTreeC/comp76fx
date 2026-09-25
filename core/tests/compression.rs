//! Does the circuit compress the way an 1176 does?
//!
//! The ratio is measured the way you would measure it on the bench: feed a
//! steady tone, let the loop settle, and look at how far the output moves for
//! a given move at the input.

use comp76fx_core::dsp::{self, Channel, Controls, Revision};
use comp76fx_core::params::dial_position;

const FS: f64 = 96_000.0;

/// Silenced for measurement; noise is checked separately.
const REV_D: Revision = dsp::REV_D.without_noise();

fn buttons(index: usize) -> [bool; 4] {
    let mut buttons = [false; 4];
    buttons[index] = true;
    buttons
}

/// Amplitude of the 1 kHz fundamental at the output, in dBFS, once settled.
fn steady_output_db(controls: Controls, input_db: f64) -> f64 {
    let mut channel = Channel::new(REV_D, FS, 4, 1);
    channel.set_controls(controls);

    let amplitude = 10.0_f64.powf(input_db / 20.0);
    let w = std::f64::consts::TAU * 1000.0 / FS;

    // Long enough for the slowest release to have settled.
    for n in 0..(FS as usize * 3) {
        channel.process((amplitude * (w * n as f64).sin()) as f32);
    }

    let window = FS as usize;
    let (mut re, mut im) = (0.0, 0.0);
    for n in 0..window {
        let phase = w * n as f64;
        let y = channel.process((amplitude * phase.sin()) as f32) as f64;
        re += y * phase.sin();
        im += y * phase.cos();
    }
    let magnitude = 2.0 * (re * re + im * im).sqrt() / window as f64;
    20.0 * (magnitude + 1e-15).log10()
}

/// The slope of output against input, well above the operating point.
fn measured_ratio(controls: Controls) -> f64 {
    let low = steady_output_db(controls, -20.0);
    let high = steady_output_db(controls, -10.0);
    10.0 / (high - low)
}

#[test]
fn the_ratio_buttons_do_what_they_say() {
    for (index, marked) in [4.0, 8.0, 12.0, 20.0].into_iter().enumerate() {
        let controls = Controls {
            buttons: buttons(index),
            ..Controls::default()
        };
        let measured = measured_ratio(controls);
        println!("marked {marked:>4}:1   measured {measured:>6.2}:1");
        // A feedback compressor's ratio is set by the loop gain, so it lands
        // on the marking rather than near it.
        assert!(
            (measured - marked).abs() < marked * 0.12,
            "the {marked}:1 button measured {measured:.2}:1"
        );
    }
}

#[test]
fn no_buttons_means_no_gain_reduction() {
    let controls = Controls {
        buttons: [false; 4],
        ..Controls::default()
    };
    // The manual is explicit that this passes the signal with colour but no
    // gain reduction at all.
    // Measured low enough that the output stage is not yet saturating, so
    // this isolates the gain element rather than the amplifier's colour.
    let quiet = steady_output_db(controls, -50.0);
    let loud = steady_output_db(controls, -30.0);
    println!("1:1  -50 dB in -> {quiet:.2}   -30 dB in -> {loud:.2}");
    assert!(
        (loud - quiet - 20.0).abs() < 0.5,
        "expected the level to track the input, got {:.2} dB for 20 dB in",
        loud - quiet
    );
}

#[test]
fn all_buttons_lands_between_twelve_and_twenty() {
    // Measured driven, which is how the mode is used. The manual's own figure
    // is loose -- "somewhere between" -- and the model is fitted to its
    // middle; see `DEAD_ZONE_LOOP_GAIN`.
    let controls = Controls {
        buttons: [true; 4],
        input_db: 20.0,
        ..Controls::default()
    };
    let measured = measured_ratio(controls);
    println!("all buttons in   measured {measured:.2}:1");
    assert!(
        (12.0..=20.0).contains(&measured),
        "all-button mode measured {measured:.2}:1"
    );
}

/// Time for the gain reduction to reach 63 % of where it settles, which is
/// the time constant the panel is marked in.
fn attack_time_ms(attack_knob: f64) -> f64 {
    let controls = Controls {
        attack: attack_knob,
        buttons: buttons(3),
        ..Controls::default()
    };
    let amplitude = 10.0_f64.powf(-6.0 / 20.0);
    let w = std::f64::consts::TAU * 3000.0 / FS;

    // Where the reduction ends up for this tone.
    let settled = {
        let mut channel = Channel::new(REV_D, FS, 1, 1);
        channel.set_controls(controls);
        for n in 0..(FS as usize / 2) {
            channel.process((amplitude * (w * n as f64).sin()) as f32);
        }
        channel.gain_reduction_db()
    };

    // A step from silence into that tone, watching the reduction build.
    let mut channel = Channel::new(REV_D, FS, 1, 1);
    channel.set_controls(controls);
    let target = settled * 0.63;
    for n in 0..(FS as usize / 10) {
        channel.process((amplitude * (w * n as f64).sin()) as f32);
        if channel.gain_reduction_db() >= target {
            return n as f64 / FS * 1000.0;
        }
    }
    f64::INFINITY
}

#[test]
fn attack_spans_the_specified_range() {
    // The manual marks 20 microseconds fully clockwise to 800 microseconds
    // fully anticlockwise, and those are the attack network's own time
    // constants, which is what the detector is set to.
    //
    // What is measured here is not the same quantity. In a feedback loop the
    // sidechain demands far more reduction than the loop settles at, so the
    // envelope passes 63 % of its settling point well before one time
    // constant has elapsed. The figures below are that closed loop behaviour,
    // and the span between them is what the knob is worth in use.
    //
    // The fastest setting is held up by the tone rather than by the knob.
    // Starting from a zero crossing, a 3 kHz sine takes about 26 us to rise
    // far enough to call for 63 % of the reduction it settles at, so no
    // detector can get there sooner. This test once demanded a tenfold span
    // and the fastest setting met it in a single sample -- by overshooting
    // the settling point by 20 dB, which was the bug, not the attack.
    let fastest = attack_time_ms(1.0);
    let slowest = attack_time_ms(0.0);
    println!("attack   fastest {fastest:.3} ms   slowest {slowest:.3} ms");
    assert!(
        (0.025..0.05).contains(&fastest),
        "fastest attack was {fastest:.3} ms"
    );
    assert!(
        (0.15..0.6).contains(&slowest),
        "slowest attack was {slowest:.3} ms"
    );
    assert!(
        slowest > fastest * 5.0,
        "the attack knob spans only {:.1}x",
        slowest / fastest
    );
}

/// The deepest reduction reached in the first few milliseconds of a tone that
/// comes in at its peak, against the deepest once it has settled, in dB.
///
/// The tone is faded in over a tenth of a millisecond rather than switched
/// on. A tone switched on at its peak is a step, and the oversampler's
/// reconstruction of a step rings about a decibel past it, as any band
/// limited step does; the fastest attack rightly catches that. A fade this
/// short is still a hard transient -- five samples at 48 kHz -- but it is one
/// the band can carry, so what is left is the loop's own behaviour.
fn attack_overshoot_db(buttons: [bool; 4], fs: f64, factor: usize) -> f64 {
    let controls = Controls {
        buttons,
        input_db: 20.0,
        attack: 1.0,
        ..Controls::default()
    };
    let mut channel = Channel::new(REV_D, fs, factor, 1);
    channel.set_controls(controls);
    let w = std::f64::consts::TAU * 1000.0 / fs;
    let total = (fs * 1.5) as usize;
    let fade = (fs * 0.0001) as usize;
    let (mut early, mut late) = (0.0f64, 0.0f64);
    for n in 0..total {
        let gain = if n < fade {
            0.5 - 0.5 * (std::f64::consts::PI * n as f64 / fade as f64).cos()
        } else {
            1.0
        };
        channel.process((0.1 * gain * (w * n as f64).cos()) as f32);
        let reduction = channel.gain_reduction_db();
        if n < (fs * 0.005) as usize {
            early = early.max(reduction);
        }
        if n >= total - (fs * 0.02) as usize {
            late = late.max(reduction);
        }
    }
    early - late
}

/// A transient must not pull the gain further down than the tone behind it
/// settles at.
///
/// The loop used to run one sample behind itself, and at the fastest attack
/// that step overshot: without oversampling this transient dug up to 20 dB
/// past the settling point and held it for the whole release, and at 2x it
/// was still 3 dB. The attack test above starts at a zero crossing and only
/// looks for the 63 % point, so it never saw it.
#[test]
fn a_fast_attack_does_not_overshoot() {
    for (fs, factor) in [(44_100.0, 1), (48_000.0, 1), (48_000.0, 2), (48_000.0, 4)] {
        for index in 0..4 {
            let overshoot = attack_overshoot_db(buttons(index), fs, factor);
            println!("{fs} Hz {factor}x, button {index}: overshoot {overshoot:+.2} dB");
            assert!(
                overshoot < 0.5,
                "button {index} at {fs} Hz, {factor}x overshot by {overshoot:.1} dB"
            );
        }
    }
}

/// The same, for the recovery.
fn release_time_ms(release_knob: f64) -> f64 {
    let controls = Controls {
        release: release_knob,
        buttons: buttons(3),
        ..Controls::default()
    };
    let amplitude = 10.0_f64.powf(-6.0 / 20.0);
    let w = std::f64::consts::TAU * 3000.0 / FS;

    let mut channel = Channel::new(REV_D, FS, 1, 1);
    channel.set_controls(controls);
    for n in 0..(FS as usize / 2) {
        channel.process((amplitude * (w * n as f64).sin()) as f32);
    }
    let from = channel.gain_reduction_db();

    // Silence, and watch it recover 63 % of the way back.
    let target = from * (1.0 - 0.63);
    for n in 0..(FS as usize * 3) {
        channel.process(0.0);
        if channel.gain_reduction_db() <= target {
            return n as f64 / FS * 1000.0;
        }
    }
    f64::INFINITY
}

#[test]
fn release_spans_the_specified_range() {
    // The manual marks 50 milliseconds fully clockwise to 1.1 seconds fully
    // anticlockwise. The recovery runs two stages together, so the measured
    // time sits above the fast stage's own constant, and the same closed loop
    // caveat as the attack applies.
    let fastest = release_time_ms(1.0);
    let slowest = release_time_ms(0.0);
    println!("release  fastest {fastest:.1} ms   slowest {slowest:.1} ms");
    assert!(
        (40.0..250.0).contains(&fastest),
        "fastest release was {fastest:.1} ms"
    );
    assert!(
        (900.0..4000.0).contains(&slowest),
        "slowest release was {slowest:.1} ms"
    );
    assert!(slowest > fastest * 5.0, "the release knob barely moved");
}

#[test]
fn the_revisions_are_audibly_different() {
    let controls = Controls {
        input_db: 20.0,
        buttons: buttons(3),
        ..Controls::default()
    };

    // Harmonic distortion of each revision on a hard driven tone.
    let distortion = |revision: Revision| {
        let mut channel = Channel::new(revision, FS, 4, 1);
        channel.set_controls(controls);
        let w = std::f64::consts::TAU * 1000.0 / FS;
        for n in 0..(FS as usize) {
            channel.process((0.25 * (w * n as f64).sin()) as f32);
        }

        let window = FS as usize;
        let mut samples = Vec::with_capacity(window);
        for n in 0..window {
            samples.push(channel.process((0.25 * (w * n as f64).sin()) as f32) as f64);
        }
        let bin = |harmonic: f64| {
            let (mut re, mut im) = (0.0, 0.0);
            for (n, y) in samples.iter().enumerate() {
                let phase = w * harmonic * n as f64;
                re += y * phase.sin();
                im += y * phase.cos();
            }
            (re * re + im * im).sqrt() / window as f64
        };
        let fundamental = bin(1.0);
        let harmonics = (2..=5).map(|h| bin(h as f64).powi(2)).sum::<f64>().sqrt();
        20.0 * (harmonics / fundamental).log10()
    };

    let rev_a = distortion(dsp::REV_A.without_noise());
    let rev_d = distortion(REV_D);
    let rev_f = distortion(dsp::REV_F.without_noise());

    println!("distortion   Rev A {rev_a:.1} dB   Rev D {rev_d:.1} dB   Rev F {rev_f:.1} dB");
    assert!(rev_a > rev_d, "Rev A should be dirtier than Rev D");
    assert!(rev_d > rev_f, "Rev D should be dirtier than Rev F");
}

/// The built-in preset should be what its name says: all four ratio switches
/// in, which is the only way to reach all-button mode.
#[test]
fn the_all_buttons_in_preset_really_is() {
    let dials = comp76fx_core::presets::built_in_dials("All Buttons In")
        .expect("the All Buttons In preset should exist");

    let dial = |id: &str| {
        dials
            .iter()
            .find(|(name, _)| *name == id)
            .map(|(_, value)| *value)
            .unwrap_or_else(|| panic!("preset is missing {id}"))
    };

    for switch in ["ratio4", "ratio8", "ratio12", "ratio20"] {
        assert_eq!(dial(switch), 1.0, "{switch} should be in");
    }

    let controls = Controls {
        input_db: dial("input") as f64,
        output_db: dial("output") as f64,
        attack: dial_position(dial("attack")),
        release: dial_position(dial("release")),
        limiting: true,
        buttons: [true; 4],
    };
    assert!(
        controls.all_buttons(),
        "the preset must reach all-button mode"
    );

    // The ratio the switches select is checked elsewhere, at levels where the
    // sidechain is still in its linear region. This preset deliberately drives
    // far past that, so what matters here is that it is genuinely limiting.

    // Driven hard enough to actually be working.
    let mut channel = Channel::new(REV_D, FS, 4, 1);
    channel.set_controls(controls);
    let w = std::f64::consts::TAU * 1000.0 / FS;
    for n in 0..(FS as usize / 2) {
        channel.process((0.2 * (w * n as f64).sin()) as f32);
    }
    let reduction = channel.gain_reduction_db();
    println!("preset gain reduction {reduction:.1} dB");
    assert!(
        reduction > 8.0,
        "the preset should be well into gain reduction, got {reduction:.1} dB"
    );
}

/// All-button mode has to stay the dirty one.
///
/// How dirty is a voicing decision and the constant behind it gets turned; the
/// direction is not. This once ran *cleaner* than a plain 4:1 at the same
/// settings, which is backwards, and nothing caught it.
#[test]
fn all_buttons_is_dirtier_than_a_plain_ratio() {
    let distortion = |controls: Controls| {
        let mut channel = Channel::new(REV_D, FS, 4, 1);
        channel.set_controls(controls);
        let amplitude = 10.0_f64.powf(-18.0 / 20.0);
        let w = std::f64::consts::TAU * 1000.0 / FS;
        for n in 0..(FS as usize * 2) {
            channel.process((amplitude * (w * n as f64).sin()) as f32);
        }
        let window = FS as usize;
        let mut samples = Vec::with_capacity(window);
        for n in 0..window {
            samples.push(channel.process((amplitude * (w * n as f64).sin()) as f32) as f64);
        }
        let bin = |harmonic: f64| {
            let (mut re, mut im) = (0.0, 0.0);
            for (n, y) in samples.iter().enumerate() {
                let phase = w * harmonic * n as f64;
                re += y * phase.sin();
                im += y * phase.cos();
            }
            (re * re + im * im).sqrt() / window as f64
        };
        let fundamental = bin(1.0);
        let harmonics = (2..=6).map(|h| bin(h as f64).powi(2)).sum::<f64>().sqrt();
        100.0 * harmonics / fundamental
    };

    let driven = Controls {
        input_db: 16.0,
        ..Controls::default()
    };
    let all = distortion(Controls {
        buttons: [true; 4],
        ..driven
    });
    let four = distortion(Controls {
        buttons: buttons(0),
        ..driven
    });
    println!("at the same settings: all buttons in {all:.2} %, 4:1 {four:.2} %");

    assert!(
        all > four * 1.4,
        "all-button mode should be plainly the dirtier one: {all:.2} % against {four:.2} %"
    );
    // And not so dirty that it has stopped being a compressor.
    assert!(
        all < 6.0,
        "all-button mode at {all:.2} % is a fuzz box, not an 1176"
    );
}

/// The built-in presets are set so a tone at -18 dBFS comes back at the level
/// it went in, whatever they are doing to it. Anything that moves how hard a
/// ratio works -- its threshold, its knee -- moves them off unity, which this
/// catches. The parallel preset is a blend by design and is left out.
#[test]
fn the_built_in_presets_come_back_at_unity() {
    for name in ["Vocal 4:1", "All Buttons In", "Bass 8:1"] {
        let dials = comp76fx_core::presets::built_in_dials(name).expect("a built-in preset");
        let dial = |id: &str| {
            dials
                .iter()
                .find(|(name, _)| *name == id)
                .map(|(_, value)| *value)
                .unwrap_or_else(|| panic!("{name} is missing {id}"))
        };
        let controls = Controls {
            input_db: dial("input") as f64,
            output_db: dial("output") as f64,
            attack: dial_position(dial("attack")),
            release: dial_position(dial("release")),
            limiting: true,
            buttons: [
                dial("ratio4") > 0.5,
                dial("ratio8") > 0.5,
                dial("ratio12") > 0.5,
                dial("ratio20") > 0.5,
            ],
        };
        let gain = steady_output_db(controls, -18.0) + 18.0;
        println!("{name}: {gain:+.2} dB at -18 dBFS");
        assert!(
            gain.abs() < 0.25,
            "{name} comes back {gain:+.2} dB off unity"
        );
    }
}

/// Turned fully anticlockwise, the attack control switches the limiting off:
/// "signal continues to pass through the 1176LN circuitry. This is commonly
/// used to add the 'color' of the 1176LN without any actual gain reduction."
/// So with a ratio selected and the unit driven hard, nothing is reduced and
/// the level tracks the input.
#[test]
fn the_attack_off_position_passes_colour_only() {
    let off = Controls {
        buttons: buttons(3),
        limiting: false,
        ..Controls::default()
    };
    let quiet = steady_output_db(off, -50.0);
    let loud = steady_output_db(off, -30.0);
    assert!(
        (loud - quiet - 20.0).abs() < 0.5,
        "with the limiting off the level moved {:.2} dB for 20 dB in",
        loud - quiet
    );
    let mut channel = Channel::new(REV_D, FS, 4, 1);
    channel.set_controls(Controls {
        input_db: 20.0,
        ..off
    });
    for n in 0..(FS as usize / 2) {
        let w = std::f64::consts::TAU * 1000.0 / FS;
        channel.process((0.3 * (w * n as f64).sin()) as f32);
    }
    assert_eq!(channel.gain_reduction_db(), 0.0);
}

/// Harmonic distortion of a 1 kHz tone at the output, in percent.
fn thd_percent(controls: Controls, input_db: f64) -> f64 {
    let mut channel = Channel::new(REV_D, FS, 4, 1);
    channel.set_controls(controls);
    let amplitude = 10.0_f64.powf(input_db / 20.0);
    let w = std::f64::consts::TAU * 1000.0 / FS;
    for n in 0..FS as usize {
        channel.process((amplitude * (w * n as f64).sin()) as f32);
    }
    let window = FS as usize;
    let samples: Vec<f64> = (0..window)
        .map(|n| channel.process((amplitude * (w * n as f64).sin()) as f32) as f64)
        .collect();
    let bin = |harmonic: f64| {
        let (mut re, mut im) = (0.0, 0.0);
        for (n, y) in samples.iter().enumerate() {
            let phase = w * harmonic * n as f64;
            re += y * phase.sin();
            im += y * phase.cos();
        }
        (re * re + im * im).sqrt()
    };
    let harmonics = (2..=6).map(|h| bin(h as f64).powi(2)).sum::<f64>().sqrt();
    100.0 * harmonics / bin(1.0)
}

/// The output control sits between the preamplifier and the line amplifier,
/// so turning it up drives the output stage harder: the same compressed
/// signal comes out more coloured with the make-up raised. When the control
/// was a plain gain after everything, the colour did not move with it.
#[test]
fn the_output_control_drives_the_line_amplifier() {
    let controls = |output_db| Controls {
        buttons: buttons(3),
        output_db,
        ..Controls::default()
    };
    let low = thd_percent(controls(0.0), -6.0);
    let high = thd_percent(controls(18.0), -6.0);
    println!("20:1, tone at -6 dBFS: output 0 dB {low:.3} %, output +18 dB {high:.3} %");
    assert!(
        high > low * 1.5,
        "raising the output from 0 to +18 dB took the distortion from {low:.3} % to {high:.3} %"
    );
}
