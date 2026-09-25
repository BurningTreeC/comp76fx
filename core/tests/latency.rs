//! The plugin's timing against the rest of a session: the latency it reports
//! has to be the latency it has, at every oversampling setting, and the dry
//! signal it blends back in has to arrive with the wet one.
//!
//! Both were wrong. The latency was reported as the 8x figure whatever the
//! setting, so the host lined the track up 74 samples early with the
//! oversampling off; and the dry signal came straight from the input, so the
//! blend combed against the wet one.

use comp76fx_core::dsp::{Channel, Controls, Revision, LATENCY, REV_D};
use comp76fx_core::plugin::Strip;

const REV: Revision = REV_D.without_noise();
const FACTORS: [usize; 4] = [1, 2, 4, 8];

/// Nothing pressed, so what comes out is the circuit's own colour and timing.
const OUT_OF_CIRCUIT: Controls = Controls {
    input_db: 0.0,
    output_db: 0.0,
    attack: 0.5,
    release: 0.5,
    limiting: true,
    buttons: [false; 4],
};

/// Where the largest sample of an impulse's response lands.
fn impulse_peak(mut process: impl FnMut(f32) -> f32) -> usize {
    let mut best = (0, 0.0f32);
    for n in 0..400 {
        let y = process(if n == 0 { 0.01 } else { 0.0 }).abs();
        if y > best.1 {
            best = (n, y);
        }
    }
    best.0
}

#[test]
fn the_reported_latency_is_the_real_latency_at_every_setting() {
    for factor in FACTORS {
        let mut channel = Channel::new(REV, 48_000.0, factor, 1);
        channel.set_controls(OUT_OF_CIRCUIT);
        assert_eq!(channel.latency(), LATENCY);
        let peak = impulse_peak(|x| channel.process(x));
        assert_eq!(
            peak, LATENCY as usize,
            "at {factor}x the circuit's output peaks {peak} samples late, and \
             the host is told {LATENCY}"
        );
    }
}

/// Changing the setting while playing keeps the same latency.
#[test]
fn switching_oversampling_keeps_the_latency() {
    let mut channel = Channel::new(REV, 48_000.0, 1, 1);
    channel.set_controls(OUT_OF_CIRCUIT);
    for factor in [8, 2, 4, 1] {
        channel.set_oversampling(factor);
        assert_eq!(impulse_peak(|x| channel.process(x)), LATENCY as usize);
    }
}

/// Gain of a tone through a strip at a blend, in dB, once settled.
fn blended_gain_db(factor: usize, hz: f64, mix: f32) -> f64 {
    let fs = 48_000.0;
    let mut strip = Strip::new(REV, fs, factor, 1);
    strip.set_controls(OUT_OF_CIRCUIT);
    let amplitude = 0.05;
    let w = std::f64::consts::TAU * hz / fs;
    let settle = (fs * 0.3) as usize;
    let window = fs as usize;
    let (mut re, mut im) = (0.0, 0.0);
    for n in 0..settle + window {
        let phase = w * n as f64;
        let y = strip.process((amplitude * phase.sin()) as f32, mix, true) as f64;
        if n >= settle {
            re += y * phase.sin();
            im += y * phase.cos();
        }
    }
    20.0 * ((2.0 * (re * re + im * im).sqrt() / window as f64) / amplitude).log10()
}

/// Half dry and half wet, with nothing being compressed, is the same signal
/// twice and has to come out flat. With the dry signal early it had notches
/// 44 dB deep at 333 Hz and 62 dB deep at 1 kHz at the default 4x.
#[test]
fn a_dry_blend_does_not_comb() {
    for factor in FACTORS {
        for hz in [100.0, 333.3, 1000.0, 5000.0, 12_000.0] {
            let gain = blended_gain_db(factor, hz, 0.5);
            assert!(
                gain.abs() < 0.6,
                "{hz} Hz at {factor}x through a half blend: {gain:+.2} dB"
            );
        }
    }
}

/// With the power off the dry signal still comes out where the host expects
/// it, so switching the unit out does not move the track in time.
#[test]
fn switched_off_the_dry_signal_keeps_its_place() {
    for factor in FACTORS {
        let mut strip = Strip::new(REV, 48_000.0, factor, 1);
        let peak = impulse_peak(|x| strip.process(x, 1.0, false));
        assert_eq!(peak, LATENCY as usize, "at {factor}x switched off");
    }
}
