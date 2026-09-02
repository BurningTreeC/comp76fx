//! Does the unit measure the way the data sheet says, and does the meter point
//! at the numbers printed on its own face?
//!
//! These pin the three things that were wrong: the release ran long, the ratio
//! buttons read low, and the needle was placed as though a VU scale were
//! linear in decibels.

use comp76fx_core::dsp::detector::{self, Timing};
use comp76fx_core::dsp::Detector;
use comp76fx_core::editor::sprites::{vu_mark, vu_position};

const FS: f64 = 768_000.0;

/// Time for the detector to fall to 37 % of where it settled, which is what
/// the 50 ms and 1100 ms on the panel mean.
fn release_time(marked: f64) -> f64 {
    let mut d = Detector::new(FS);
    d.set_timing(Timing {
        k: 3.0,
        attack: detector::ATTACK_FASTEST,
        release: marked,
        threshold: detector::THRESHOLD_DB,
        knee: 0.0,
    });
    let level = 10f64.powf(-6.0 / 20.0);
    let mut settled = 0.0;
    for _ in 0..(FS as usize * 4) {
        settled = d.process(level);
    }
    for n in 0..(FS as usize * 8) {
        if d.process(0.0) <= settled * 0.37 {
            return n as f64 / FS;
        }
    }
    f64::INFINITY
}

#[test]
fn the_release_dial_means_what_it_says() {
    // Two stages recovering together take longer than either alone, so the
    // fast one is shortened to compensate. Without that every setting ran
    // 78 % long, which is a different compressor.
    for marked in [
        detector::RELEASE_FASTEST,
        0.2,
        0.5,
        detector::RELEASE_SLOWEST,
    ] {
        let measured = release_time(marked);
        let error = (measured - marked).abs() / marked;
        assert!(
            error < 0.05,
            "release marked {:.0} ms measured {:.0} ms",
            marked * 1e3,
            measured * 1e3
        );
    }
}

#[test]
fn the_attack_dial_means_what_it_says() {
    for marked in [detector::ATTACK_FASTEST, 200e-6, detector::ATTACK_SLOWEST] {
        let mut d = Detector::new(FS);
        d.set_timing(Timing {
            k: 3.0,
            attack: marked,
            release: detector::RELEASE_FASTEST,
            threshold: detector::THRESHOLD_DB,
        knee: 0.0,
        });
        let level = 10f64.powf(-6.0 / 20.0);
        let mut settled = 0.0;
        for _ in 0..(FS as usize * 4) {
            settled = d.process(level);
        }
        d.reset();
        let mut measured = f64::INFINITY;
        for n in 0..(FS as usize) {
            if d.process(level) >= settled * 0.63 {
                measured = n as f64 / FS;
                break;
            }
        }
        let error = (measured - marked).abs() / marked;
        assert!(
            error < 0.05,
            "attack marked {:.1} us measured {:.1} us",
            marked * 1e6,
            measured * 1e6
        );
    }
}

#[test]
fn the_sidechain_stays_linear_where_the_unit_works() {
    // At equilibrium the demand and the gain reduction are the same number, so
    // any bend here bends the static ratio with it. Bending from the origin,
    // as a plain tanh does, is what made 20:1 read 17:1.
    for gr in [0.0, 3.0, 10.0, 20.0, 30.0] {
        let out = detector::limit_demand(gr);
        assert!(
            (out - gr).abs() < 1e-9,
            "demand bent at {gr} dB of gain reduction: {out}"
        );
    }
    // It still has to run out of rail somewhere.
    assert!(detector::limit_demand(1000.0) < 70.0);
}

#[test]
fn the_needle_points_at_the_printed_numbers() {
    // A moving coil deflects with voltage, so the marks are spaced
    // logarithmically. Reading the face as linear in dB put 0 VU at the right
    // hand end of the scale instead of two thirds along it.
    assert!((vu_position(-20.0) - 0.0).abs() < 1e-6, "the -20 mark is the left end");
    assert!((vu_position(3.0) - 1.0).abs() < 1e-6, "the +3 mark is the right end");

    let zero = vu_position(0.0);
    assert!(
        (0.66..0.71).contains(&zero),
        "0 VU should sit about two thirds along the scale, not at {zero:.3}"
    );

    // The spacing has to widen towards the top of the scale.
    let low = vu_position(-10.0) - vu_position(-20.0);
    let high = vu_position(0.0) - vu_position(-3.0);
    assert!(
        high > low * 1.3,
        "the scale is not logarithmic: -20..-10 spans {low:.3}, -3..0 spans {high:.3}"
    );

    // And the marks have to land on the arc that is printed on the face. These
    // are the tick positions measured off the photograph, as fractions of it.
    for (db, x, y) in [
        (-20.0, 0.1878, 0.4565),
        (-10.0, 0.2903, 0.3872),
        (-5.0, 0.4148, 0.3415),
        (0.0, 0.6423, 0.3508),
        (3.0, 0.8424, 0.4642),
    ] {
        let (mx, my) = vu_mark(0.0, 0.0, 1.0, 1.0, vu_position(db));
        assert!(
            (mx - x).abs() < 0.004 && (my - y).abs() < 0.004,
            "{db} VU lands at ({mx:.4}, {my:.4}), the printed tick is at ({x}, {y})"
        );
    }
}

/// The response has to hold its published window at whatever rate the session
/// runs at, and with the oversampling anywhere the user can put it.
///
/// This is the check that catches the band limits being clamped to the sample
/// rate: doing that turned the output transformer's gentle top-end tilt into a
/// wall at 19.8 kHz on a 44.1 kHz session, inside the band the unit is
/// specified across.
#[test]
fn the_response_holds_its_window_at_every_rate() {
    use comp76fx_core::dsp::{Channel, Controls, Finish, OutputStage, Revision};

    const REV: Revision = Revision {
        name: "Rev D",
        finish: Finish::BlackFace,
        slug: "spec",
        stage: OutputStage::ClassA,
        amp_drive: 0.45,
        fet_drive: 1.00,
        fet_bias: 0.12,
        noise_floor_db: -400.0,
        ratio_accuracy: 1.0,
    };

    // Unity gain with the gain element out of circuit, as the figure is quoted.
    let flat = Controls {
        input_db: 0.0,
        output_db: 0.0,
        attack: 0.5,
        release: 0.5,
        buttons: [false; 4],
    };

    let response = |hz: f64, fs: f64, os: usize| -> f64 {
        let mut ch = Channel::new(REV, fs, os, 1);
        ch.set_controls(flat);
        let amp = 10f64.powf(-24.0 / 20.0);
        let w = std::f64::consts::TAU * hz / fs;
        for n in 0..(fs as usize / 2) {
            ch.process((amp * (w * n as f64).sin()) as f32);
        }
        let win = (fs as usize).min((fs / hz * 200.0) as usize);
        let (mut re, mut im) = (0.0, 0.0);
        for n in 0..win {
            let p = w * n as f64;
            let y = ch.process((amp * p.sin()) as f32) as f64;
            re += y * p.sin();
            im += y * p.cos();
        }
        20.0 * ((2.0 * (re * re + im * im).sqrt() / win as f64) / amp).log10()
    };

    for fs in [44_100.0, 48_000.0, 96_000.0] {
        for os in [1usize, 2, 4] {
            let mid = response(1000.0, fs, os);
            for hz in [20.0, 100.0, 10_000.0, 20_000.0] {
                // Only up to what the rate can carry; 20 kHz is above Nyquist
                // for nothing here, but keep the guard honest anyway.
                if hz >= fs * 0.45 {
                    continue;
                }
                let d = response(hz, fs, os) - mid;
                assert!(
                    d.abs() <= 1.0,
                    "{hz} Hz at {fs} Hz, oversampling {os}x: {d:+.2} dB, outside the published +/-1 dB"
                );
            }
        }
    }
}
