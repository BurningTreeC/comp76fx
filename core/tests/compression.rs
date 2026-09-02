//! Does the circuit compress the way an 1176 does?
//!
//! The ratio is measured the way you would measure it on the bench: feed a
//! steady tone, let the loop settle, and look at how far the output moves for
//! a given move at the input.

use comp76fx_core::dsp::{Channel, Controls, Finish, OutputStage, Revision};

const FS: f64 = 96_000.0;

const REV_D: Revision = Revision {
    name: "Rev D",
    finish: Finish::BlackFace,
    slug: "comp76fx-rev-d",
    stage: OutputStage::ClassA,
    amp_drive: 0.45,
    fet_drive: 1.0,
    fet_bias: 0.12,
    // Silenced for measurement; noise is checked separately.
    noise_floor_db: -400.0,
    ratio_accuracy: 1.0,
};

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
    let fastest = attack_time_ms(1.0);
    let slowest = attack_time_ms(0.0);
    println!("attack   fastest {fastest:.3} ms   slowest {slowest:.3} ms");
    assert!(fastest < 0.05, "fastest attack was {fastest:.3} ms");
    assert!(
        (0.15..0.6).contains(&slowest),
        "slowest attack was {slowest:.3} ms"
    );
    assert!(
        slowest > fastest * 10.0,
        "the attack knob spans only {:.0}x",
        slowest / fastest
    );
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

    let rev_a = distortion(Revision {
        amp_drive: 0.62,
        fet_drive: 1.55,
        fet_bias: 0.30,
        noise_floor_db: -400.0,
        ratio_accuracy: 0.88,
        ..REV_D
    });
    let rev_d = distortion(REV_D);
    let rev_f = distortion(Revision {
        stage: OutputStage::ClassAb,
        amp_drive: 0.34,
        fet_drive: 0.62,
        fet_bias: 0.04,
        ..REV_D
    });

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
        attack: (dial("attack") / 7.0) as f64,
        release: (dial("release") / 7.0) as f64,
        buttons: [true; 4],
    };
    assert!(controls.all_buttons(), "the preset must reach all-button mode");

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
