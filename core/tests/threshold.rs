//! The threshold circuit and the ratio switch bank, held to the 1176LN manual
//! and the Rev D schematic.
//!
//! The ratio buttons switch two ladders at once: the one that feeds the
//! sidechain, which sets the ratio, and one in the bias network, which sets
//! the threshold and where the gate rests. So each ratio has a threshold of
//! its own, and the diodes' gradual turn-on gives each a knee, softest at
//! 4:1. The sidechain takes its signal from the preamplifier, after the gain
//! element and before the output control and line amplifier. Pressing several
//! buttons shorts both ladders between the outermost two, which is all that
//! all-button mode is.

use comp76fx_core::dsp::{
    detector, Channel, Controls, OutputStage, Revision, REV_D, REV_F, THRESHOLD_OFFSETS_DB,
};

const FS: f64 = 48_000.0;
/// The plugin's default oversampling.
const FACTOR: usize = 4;
const MARKED: [f64; 4] = [4.0, 8.0, 12.0, 20.0];

fn button(index: usize) -> [bool; 4] {
    let mut buttons = [false; 4];
    buttons[index] = true;
    buttons
}

/// A steady tone through a channel: the mean gain reduction the meter would
/// show and the level of the fundamental at the output, both in dB.
fn measure(revision: Revision, controls: Controls, hz: f64, peak_db: f64) -> (f64, f64) {
    let mut channel = Channel::new(revision.without_noise(), FS, FACTOR, 1);
    channel.set_controls(controls);
    let amplitude = 10f64.powf(peak_db / 20.0);
    let w = std::f64::consts::TAU * hz / FS;
    let settle = (FS * 0.6) as usize;
    let window = (FS * 0.2) as usize;
    let (mut reduction, mut re, mut im) = (0.0, 0.0, 0.0);
    for n in 0..settle + window {
        let phase = w * n as f64;
        let y = channel.process((amplitude * phase.sin()) as f32) as f64;
        if n >= settle {
            reduction += channel.gain_reduction_db();
            re += y * phase.sin();
            im += y * phase.cos();
        }
    }
    let magnitude = 2.0 * (re * re + im * im).sqrt() / window as f64;
    (
        reduction / window as f64,
        20.0 * (magnitude + 1e-15).log10(),
    )
}

/// The manual's factory ratio test, section 13 of the calibration procedure.
///
/// A 2 kHz tone, attack fully on, release fully clockwise. Set the unit into
/// 1 dB of limiting on its own meter -- 3 dB for the 4:1, "because of the soft
/// knee in the threshold circuit for this ratio" -- then raise the input by
/// 20 dB: the output must rise 1, 1.66, 2.5 and 5 dB for 20:1, 12:1, 8:1 and
/// 4:1, each within 20 %.
fn factory_ratio_rise(revision: Revision, index: usize, limiting_db: f64) -> f64 {
    let controls = Controls {
        buttons: button(index),
        attack: 1.0,
        release: 1.0,
        ..Controls::default()
    };
    // The input level that puts the meter on the mark.
    let (mut low, mut high) = (-60.0, 0.0);
    for _ in 0..30 {
        let mid = 0.5 * (low + high);
        if measure(revision, controls, 2000.0, mid).0 < limiting_db {
            low = mid;
        } else {
            high = mid;
        }
    }
    let start = 0.5 * (low + high);
    let (_, before) = measure(revision, controls, 2000.0, start);
    let (_, after) = measure(revision, controls, 2000.0, start + 20.0);
    after - before
}

#[test]
fn the_factory_ratio_test_passes() {
    for revision in [REV_D, REV_F] {
        for (index, marked) in MARKED.into_iter().enumerate() {
            let limiting = if index == 0 { 3.0 } else { 1.0 };
            let rise = factory_ratio_rise(revision, index, limiting);
            let wanted = 20.0 / marked;
            let error = (rise - wanted) / wanted;
            println!(
                "{} {marked:>2}:1 from {limiting} dB of limiting: +{rise:.2} dB for +20 dB in, \
                 wanted {wanted:.2} ({:+.1} %)",
                revision.name,
                error * 100.0
            );
            assert!(
                error.abs() <= 0.2,
                "{} {marked}:1 rose {rise:.2} dB for 20 dB in, outside 20 % of {wanted:.2}",
                revision.name
            );
        }
    }
}

/// Why the manual measures the 4:1 from further in: from 1 dB of limiting its
/// soft knee is still pulling the reading off, by more than it pulls any other
/// ratio off from there.
#[test]
fn the_four_to_one_knee_is_why_the_manual_starts_it_deeper() {
    let error = |index: usize, limiting: f64| {
        let wanted = 20.0 / MARKED[index];
        (factory_ratio_rise(REV_D, index, limiting) - wanted).abs() / wanted
    };
    let shallow = error(0, 1.0);
    let deep = error(0, 3.0);
    println!(
        "4:1 read from 1 dB {:.1} % out, from 3 dB {:.1} % out",
        shallow * 100.0,
        deep * 100.0
    );
    assert!(
        shallow > deep * 2.0,
        "the 4:1 reads as well from 1 dB ({:.1} %) as from 3 ({:.1} %), so its knee is not soft",
        shallow * 100.0,
        deep * 100.0
    );
    for (index, marked) in MARKED.into_iter().enumerate().skip(1) {
        let other = error(index, 1.0);
        assert!(
            shallow > other,
            "the {marked}:1 is further out from 1 dB ({:.1} %) than the 4:1 ({:.1} %)",
            other * 100.0,
            shallow * 100.0
        );
    }
}

/// Where a ratio's straight line, extended back, meets no gain reduction: its
/// threshold, read the way it is read off a transfer curve, clear of the knee.
fn threshold_db(index: usize) -> f64 {
    let controls = Controls {
        buttons: button(index),
        // Slow, so the reduction barely moves between the peaks of the tone.
        attack: 0.5,
        release: 0.0,
        ..Controls::default()
    };
    let (low, high) = (-12.0, -2.0);
    let (at_low, _) = measure(REV_D, controls, 1000.0, low);
    let (at_high, _) = measure(REV_D, controls, 1000.0, high);
    let slope = (at_high - at_low) / (high - low);
    low - at_low / slope
}

/// "Selecting higher ratios also raises the threshold level": -24, -25 and
/// -26 dB at the input for 20:1, 12:1 and 8:1 in the manual's table, and the
/// 4:1 a decibel below that by the output column.
#[test]
fn the_threshold_rises_with_the_ratio() {
    let twenty = threshold_db(3);
    println!("20:1 threshold {twenty:.2} dBFS");
    assert!(
        (twenty - detector::THRESHOLD_DB).abs() < 0.5,
        "the 20:1 threshold is at {twenty:.2} dBFS"
    );
    for (index, marked) in MARKED.into_iter().enumerate().take(3) {
        let relative = threshold_db(index) - twenty;
        let wanted = THRESHOLD_OFFSETS_DB[index];
        println!("{marked:>2}:1 threshold {relative:+.2} dB from the 20:1's, wanted {wanted:+.0}");
        assert!(
            (relative - wanted).abs() < 0.3,
            "the {marked}:1 threshold is {relative:+.2} dB from the 20:1's, not {wanted:+.0}"
        );
    }
}

/// The knee, read as how much reduction there already is with the tone
/// peaking exactly at a ratio's threshold. A hard corner would have none; the
/// diodes give every button a little, and the 4:1, whose knee the manual
/// calls soft, clearly the most. Below the knee there is none at all:
/// "signals at levels below the threshold will not be affected".
#[test]
fn the_knee_softens_towards_four_to_one() {
    let controls = |index: usize| Controls {
        buttons: button(index),
        attack: 0.5,
        release: 0.0,
        ..Controls::default()
    };
    let mut previous = f64::INFINITY;
    for (index, marked) in MARKED.into_iter().enumerate() {
        let threshold = detector::THRESHOLD_DB + THRESHOLD_OFFSETS_DB[index];
        let (at, _) = measure(REV_D, controls(index), 1000.0, threshold);
        let (below, _) = measure(REV_D, controls(index), 1000.0, threshold - 4.0);
        println!("{marked:>2}:1 at its threshold {at:.2} dB of reduction, 4 dB below {below:.3}");
        assert!(
            at < previous,
            "the {marked}:1 knee is softer than a lower ratio's"
        );
        assert!(
            below < 0.01,
            "the {marked}:1 is reducing 4 dB below its threshold"
        );
        previous = at;
        if index == 0 {
            assert!(
                at > 0.5,
                "the 4:1 knee is barely soft: {at:.2} dB at threshold"
            );
        }
        if index == 3 {
            assert!(
                at < 0.3,
                "the 20:1 knee is too soft: {at:.2} dB at threshold"
            );
        }
    }
}

/// The sidechain is fed from the preamplifier, ahead of the output control and
/// the line amplifier, so the output stage cannot change how hard the unit
/// compresses: two units differing only in their output stage reduce the gain
/// identically.
#[test]
fn the_output_stage_is_outside_the_loop() {
    let clean = Revision {
        stage: OutputStage::ClassA,
        amp_drive: 0.0,
        ..REV_D
    }
    .without_noise();
    let driven = Revision {
        stage: OutputStage::ClassAb,
        amp_drive: 1.5,
        ..REV_D
    }
    .without_noise();
    let controls = Controls {
        buttons: button(1),
        input_db: 20.0,
        attack: 1.0,
        ..Controls::default()
    };
    let mut a = Channel::new(clean, FS, FACTOR, 1);
    let mut b = Channel::new(driven, FS, FACTOR, 1);
    a.set_controls(controls);
    b.set_controls(controls);
    let w = std::f64::consts::TAU * 440.0 / FS;
    for n in 0..(FS as usize / 4) {
        let x = (0.3 * (w * n as f64).sin()) as f32;
        a.process(x);
        b.process(x);
        assert_eq!(
            a.gain_reduction_db(),
            b.gain_reduction_db(),
            "the output stage changed the gain reduction at sample {n}"
        );
    }
}

/// The outputs of two channels fed the same tone, compared sample for sample.
fn identical(a: [bool; 4], b: [bool; 4]) -> bool {
    let controls = |buttons| Controls {
        buttons,
        input_db: 12.0,
        ..Controls::default()
    };
    let mut first = Channel::new(REV_D.without_noise(), FS, FACTOR, 1);
    let mut second = Channel::new(REV_D.without_noise(), FS, FACTOR, 1);
    first.set_controls(controls(a));
    second.set_controls(controls(b));
    let w = std::f64::consts::TAU * 440.0 / FS;
    (0..(FS as usize / 4)).all(|n| {
        let x = (0.3 * (w * n as f64).sin()) as f32;
        first.process(x) == second.process(x)
    })
}

/// Pressing buttons joins their contacts, so everything between the lowest
/// and the highest pressed is shorted and the buttons in between change
/// nothing -- Universal Audio: "only the 'outside' ratios are relevant".
#[test]
fn only_the_outermost_buttons_matter() {
    assert!(
        identical([true; 4], [true, false, false, true]),
        "4 + 20 is all four"
    );
    assert!(
        identical([false, true, true, true], [false, true, false, true]),
        "8 + 20 is 8 + 12 + 20"
    );
    assert!(
        identical([true, true, true, false], [true, false, true, false]),
        "4 + 12 is 4 + 8 + 12"
    );
    assert!(
        !identical([true; 4], [false, false, false, true]),
        "all four is not 20:1"
    );
}

/// "Less attenuation than standard ratios" (ioplex's simulation): the shorted
/// ladder passes the sidechain less signal and the gate rests further off,
/// so driven the same, all four reduce less than the 20:1 alone.
#[test]
fn all_buttons_reduces_less_than_twenty_to_one() {
    let controls = |buttons| Controls {
        buttons,
        ..Controls::default()
    };
    for level in [-16.0, -10.0, -4.0] {
        let (all, _) = measure(REV_D, controls([true; 4]), 1000.0, level);
        let (twenty, _) = measure(REV_D, controls(button(3)), 1000.0, level);
        println!("tone at {level} dBFS: all four {all:.1} dB, 20:1 {twenty:.1} dB");
        assert!(
            all < twenty,
            "all four reduced more than 20:1 at {level} dBFS"
        );
    }
}

/// Time from a tone's start to 1 dB of reduction, in ms.
fn onset_ms(buttons: [bool; 4], attack: f64) -> f64 {
    let mut channel = Channel::new(REV_D.without_noise(), FS, FACTOR, 1);
    channel.set_controls(Controls {
        buttons,
        attack,
        input_db: 12.0,
        ..Controls::default()
    });
    let w = std::f64::consts::TAU * 1000.0 / FS;
    (0..(FS as usize / 10))
        .find(|&n| {
            channel.process((0.25 * (w * n as f64).sin()) as f32);
            channel.gain_reduction_db() >= 1.0
        })
        .map_or(f64::INFINITY, |n| n as f64 / FS * 1000.0)
}

/// Time for the reduction to come back under 1 dB after the tone stops, in ms.
fn recovery_ms(buttons: [bool; 4], release: f64) -> f64 {
    let mut channel = Channel::new(REV_D.without_noise(), FS, FACTOR, 1);
    channel.set_controls(Controls {
        buttons,
        release,
        input_db: 12.0,
        ..Controls::default()
    });
    let w = std::f64::consts::TAU * 1000.0 / FS;
    for n in 0..(FS as usize) {
        channel.process((0.25 * (w * n as f64).sin()) as f32);
    }
    (0..(FS as usize * 4))
        .find(|_| {
            channel.process(0.0);
            channel.gain_reduction_db() < 1.0
        })
        .map_or(f64::INFINITY, |n| n as f64 / FS * 1000.0)
}

/// The gate resting lower changes the timing, as Universal Audio describes
/// the mode: "a lag time on the attack of initial transients", and "the bias
/// points change all over the circuit, thus changing the attack and release
/// times as well". The envelope has to charge through the dead zone before
/// the gain moves, and on the way down reaches it long before it would
/// have crept back to rest.
#[test]
fn all_buttons_lags_the_attack_and_hurries_the_release() {
    let (all, twenty) = ([true; 4], button(3));
    let lag = onset_ms(all, 0.0) - onset_ms(twenty, 0.0);
    println!("slowest attack: all four start {lag:.3} ms later than 20:1");
    assert!(lag > 0.2, "all four start only {lag:.3} ms later than 20:1");
    for release in [1.0, 0.5] {
        let (fast, slow) = (recovery_ms(all, release), recovery_ms(twenty, release));
        println!("release {release}: all four recover in {fast:.1} ms, 20:1 in {slow:.1} ms");
        assert!(
            fast * 3.0 < slow,
            "all four recover in {fast:.1} ms against {slow:.1}"
        );
    }
}

/// The meter reads the gate's bias against the rest it was zeroed at. With
/// the gate pulled below that the needle rests past zero -- "the meter will
/// go wild, often resting at maximum" -- and one button in reads zero.
#[test]
fn the_meter_rests_past_zero_with_all_buttons_in() {
    let resting = |buttons| {
        let mut channel = Channel::new(REV_D.without_noise(), FS, FACTOR, 1);
        channel.set_controls(Controls {
            buttons,
            ..Controls::default()
        });
        for _ in 0..4800 {
            channel.process(0.0);
        }
        channel.take_meter()
    };
    assert_eq!(resting(button(0)), 0.0);
    assert_eq!(resting(button(3)), 0.0);
    let all = resting([true; 4]);
    assert!(
        all < -20.0,
        "all four rest at {all:.1} dB, which is on the scale"
    );
}

/// Pressing or releasing buttons while the unit is working carries the
/// reduction on from where it was rather than jumping. Going from all four to
/// one used to dip the level by 30 dB, which is what the envelope was holding
/// against the dead zone.
#[test]
fn changing_buttons_mid_signal_does_not_jump() {
    for (from, to) in [
        ([true; 4], button(3)),
        (button(3), [true; 4]),
        (button(0), [true; 4]),
    ] {
        let controls = |buttons| Controls {
            buttons,
            input_db: 6.0,
            ..Controls::default()
        };
        let mut channel = Channel::new(REV_D.without_noise(), FS, FACTOR, 1);
        channel.set_controls(controls(from));
        let w = std::f64::consts::TAU * 1000.0 / FS;
        let mut n = 0usize;
        let mut most = |channel: &mut Channel, samples: usize| {
            let mut most = 0.0f64;
            for _ in 0..samples {
                channel.process((0.25 * (w * n as f64).sin()) as f32);
                n += 1;
                most = most.max(channel.gain_reduction_db());
            }
            most
        };
        let before = most(&mut channel, FS as usize);
        channel.set_controls(controls(to));
        let after = most(&mut channel, FS as usize / 100);
        most(&mut channel, FS as usize * 3);
        let settled = most(&mut channel, FS as usize / 2);
        println!(
            "{from:?} -> {to:?}: {before:.1} dB, then {after:.1} dB, settling at {settled:.1} dB"
        );
        assert!(
            after <= before.max(settled) + 1.0,
            "switching jumped to {after:.1} dB between {before:.1} and {settled:.1}"
        );
    }
}
