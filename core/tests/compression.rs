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
    // Measured driven, which is the only way the mode is ever used. Its knee
    // is wide enough that the slope is still opening out at light gain
    // reduction, so a ratio quoted for it only means anything at a stated
    // operating point -- the same caveat the manual's own loose "somewhere
    // between" carries.
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

/// The knee is what the mode is for: it should take hold later and more
/// gradually than a plain ratio, and then pull further past it.
#[test]
fn all_buttons_has_a_softer_knee_than_a_plain_ratio() {
    let all = Controls {
        buttons: [true; 4],
        input_db: 20.0,
        ..Controls::default()
    };
    let four = Controls {
        buttons: buttons(0),
        ..all
    };

    // Driven the same, the mode reduces more than the gentlest ratio does.
    let all_hard = steady_output_db(all, -20.0);
    let four_hard = steady_output_db(four, -20.0);
    assert!(
        all_hard < four_hard,
        "all buttons in should hold the level down harder: {all_hard:.2} against {four_hard:.2}"
    );

    // And the slope keeps opening out as it is driven, rather than settling on
    // one figure the way a fixed ratio does. Held against the 4:1 button
    // rather than against a number, so it measures the difference the knee
    // makes instead of whatever the loop happens to settle at.
    let spread = |c: Controls| {
        let gentle = measured_ratio(Controls { input_db: 6.0, ..c });
        let driven = measured_ratio(c);
        (gentle, driven, driven / gentle)
    };
    let (a_gentle, a_driven, a_spread) = spread(all);
    let (f_gentle, f_driven, f_spread) = spread(four);
    println!("all buttons in   {a_gentle:.2}:1 gentle, {a_driven:.2}:1 driven  ({a_spread:.3}x)");
    println!("4:1              {f_gentle:.2}:1 gentle, {f_driven:.2}:1 driven  ({f_spread:.3}x)");
    // How far each opens out, not how far apart the two figures are: a fixed
    // ratio barely moves, so it is the departure from 1 that is being compared.
    assert!(
        (a_spread - 1.0) > (f_spread - 1.0) * 5.0,
        "the knee should open out far more than a fixed ratio: {a_spread:.3}x against {f_spread:.3}x"
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
