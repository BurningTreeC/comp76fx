//! The threshold circuit, held to the 1176LN manual.
//!
//! The ratio buttons switch two dividers at once: the one that feeds the
//! sidechain, which sets the ratio, and a DC one that biases the rectifier
//! diodes, which sets the threshold. So each ratio has a threshold of its own,
//! and the diodes' gradual turn-on gives each a knee, softest at 4:1. The
//! sidechain takes its signal from the preamplifier, after the gain element
//! and before the output control and line amplifier.

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
