//! Does the unit measure the way the data sheet says, and does the meter point
//! at the numbers printed on its own face?
//!
//! These pin the things that were wrong: the release ran long, the ratio
//! buttons read low, the needle was placed as though a VU scale were linear in
//! decibels, the dials stopped short of their slow ends, and the noise floor
//! moved with the oversampling.

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
        dead_zone: 0.0,
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

/// The shape of the recovery and the compensation for it are two constants
/// in two places. Retune one and the release dial quietly stops meaning what
/// it says, so the pair is checked here as well as through the dial.
#[test]
fn release_compensation_is_solved() {
    let (share, ratio, compensation) = detector::RELEASE_SHAPE;
    let u = 1.0 / compensation;
    let left = (1.0 - share) * (-u).exp() + share * (-u / ratio).exp();
    assert!(
        (left - 0.37).abs() < 1e-4,
        "the two stages recover to {left:.5} at the marked time, not 0.37"
    );
}

/// The dials are engraved 1 to 7, and the ends of the engraving have to be the
/// ends of the range the manual gives.
#[test]
fn the_dials_reach_their_marked_ends() {
    use comp76fx_core::params::{dial_position, DIAL_MAX, DIAL_MIN};

    assert_eq!(dial_position(DIAL_MIN), 0.0, "1 is fully anticlockwise");
    assert_eq!(dial_position(DIAL_MAX), 1.0, "7 is fully clockwise");
    assert!(
        (dial_position(4.0) - 0.5).abs() < 1e-9,
        "4 is halfway round"
    );

    let time = |mark: f32, fastest: f64, slowest: f64| {
        detector::knob_to_time(dial_position(mark), fastest, slowest)
    };
    let slowest_attack = time(DIAL_MIN, detector::ATTACK_FASTEST, detector::ATTACK_SLOWEST);
    let slowest_release = time(
        DIAL_MIN,
        detector::RELEASE_FASTEST,
        detector::RELEASE_SLOWEST,
    );
    assert!(
        (slowest_attack - 800e-6).abs() < 1e-9,
        "attack at 1 is {:.0} us, the panel says 800",
        slowest_attack * 1e6
    );
    assert!(
        (slowest_release - 1.1).abs() < 1e-9,
        "release at 1 is {:.0} ms, the panel says 1100",
        slowest_release * 1e3
    );
}

/// The noise is the circuit's, so the quality switch must not move it. It
/// used to be added at the internal rate and filtered on the way down, which
/// took 3 dB off the floor for every doubling: an idle Rev A read -91, -94,
/// -97 and -100 dBFS at 1x, 2x, 4x and 8x.
#[test]
fn the_noise_floor_does_not_move_with_oversampling() {
    use comp76fx_core::dsp::{Channel, Controls, REV_A};

    let floor = |factor: usize| {
        let mut channel = Channel::new(REV_A, 48_000.0, factor, 1);
        channel.set_controls(Controls::default());
        let n = 96_000;
        let mut sum = 0.0;
        for i in 0..n * 2 {
            let y = channel.process(0.0) as f64;
            if i >= n {
                sum += y * y;
            }
        }
        10.0 * (sum / n as f64).log10()
    };
    let reference = floor(1);
    for factor in [2, 4, 8] {
        let level = floor(factor);
        println!("idle noise {factor}x: {level:.2} dBFS, 1x {reference:.2} dBFS");
        assert!(
            (level - reference).abs() < 0.5,
            "the floor at {factor}x is {level:.1} dBFS against {reference:.1} at 1x"
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
            dead_zone: 0.0,
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
    assert!(
        (vu_position(-20.0) - 0.0).abs() < 1e-6,
        "the -20 mark is the left end"
    );
    assert!(
        (vu_position(3.0) - 1.0).abs() < 1e-6,
        "the +3 mark is the right end"
    );

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
    use comp76fx_core::dsp::{Channel, Controls, Revision, REV_D};

    const REV: Revision = REV_D.without_noise();

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
        for os in [1usize, 2, 4, 8] {
            let mid = response(1000.0, fs, os);
            // 20 kHz included at 44.1 kHz: it is the edge the figure is
            // quoted to, and the top of the oversampler's passband is closest
            // to it there. A guard at 0.45 of the rate used to skip exactly
            // that case.
            for hz in [20.0, 100.0, 10_000.0, 20_000.0] {
                let d = response(hz, fs, os) - mid;
                assert!(
                    d.abs() <= 1.0,
                    "{hz} Hz at {fs} Hz, oversampling {os}x: {d:+.2} dB, outside the published +/-1 dB"
                );
            }
        }
    }
}
