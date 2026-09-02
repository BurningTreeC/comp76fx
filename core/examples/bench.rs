//! Measures the compressor the way you would on the bench, so the numbers can
//! be held against the 1176's published specification.
//!
//! Run with: cargo run --release -p comp76fx_core --example bench

use comp76fx_core::dsp::detector::{self, Timing};
use comp76fx_core::dsp::{Channel, Controls, Detector, Finish, OutputStage, Revision};

const FS: f64 = 192_000.0;

const REV_D: Revision = Revision {
    name: "Rev D",
    finish: Finish::BlackFace,
    slug: "bench",
    stage: OutputStage::ClassA,
    amp_drive: 0.45,
    fet_drive: 1.0,
    fet_bias: 0.12,
    noise_floor_db: -400.0,
    ratio_accuracy: 1.0,
};

const REV_A: Revision = Revision {
    name: "Rev A", finish: Finish::BlueStripe, slug: "bench", stage: OutputStage::ClassA,
    amp_drive: 0.62, fet_drive: 1.55, fet_bias: 0.30, noise_floor_db: -400.0, ratio_accuracy: 0.88,
};
const REV_F: Revision = Revision {
    name: "Rev F", finish: Finish::SilverFace, slug: "bench", stage: OutputStage::ClassAb,
    amp_drive: 0.34, fet_drive: 0.62, fet_bias: 0.04, noise_floor_db: -400.0, ratio_accuracy: 1.0,
};

/// THD of a 1 kHz tone through one revision at a given signal level, percent.
fn thd_rev(rev: Revision, c: Controls, input_db: f64) -> f64 {
    let mut ch = Channel::new(rev, FS, 4, 1);
    ch.set_controls(c);
    let amp = 10f64.powf(input_db / 20.0);
    let w = std::f64::consts::TAU * 1000.0 / FS;
    for n in 0..(FS as usize * 2) {
        ch.process((amp * (w * n as f64).sin()) as f32);
    }
    let win = FS as usize;
    let mut y = Vec::with_capacity(win);
    for n in 0..win {
        y.push(ch.process((amp * (w * n as f64).sin()) as f32) as f64);
    }
    let bin = |h: f64| {
        let (mut re, mut im) = (0.0, 0.0);
        for (n, &v) in y.iter().enumerate() {
            let p = w * h * n as f64;
            re += v * p.sin();
            im += v * p.cos();
        }
        2.0 * (re * re + im * im).sqrt() / win as f64
    };
    let f1 = bin(1.0);
    let harm: f64 = (2..=8).map(|h| bin(h as f64).powi(2)).sum::<f64>().sqrt();
    100.0 * harm / f1
}

/// Second and third harmonic separately, in dB below the fundamental.
fn harmonics(rev: Revision, c: Controls, input_db: f64) -> (f64, f64) {
    let mut ch = Channel::new(rev, FS, 4, 1);
    ch.set_controls(c);
    let amp = 10f64.powf(input_db / 20.0);
    let w = std::f64::consts::TAU * 1000.0 / FS;
    for n in 0..(FS as usize * 2) { ch.process((amp * (w * n as f64).sin()) as f32); }
    let win = FS as usize;
    let mut y = Vec::with_capacity(win);
    for n in 0..win { y.push(ch.process((amp * (w * n as f64).sin()) as f32) as f64); }
    let bin = |h: f64| {
        let (mut re, mut im) = (0.0, 0.0);
        for (n, &v) in y.iter().enumerate() {
            let p = w * h * n as f64;
            re += v * p.sin(); im += v * p.cos();
        }
        2.0 * (re * re + im * im).sqrt() / win as f64
    };
    let f1 = bin(1.0);
    (20.0*(bin(2.0)/f1).log10(), 20.0*(bin(3.0)/f1).log10())
}

fn buttons(index: usize) -> [bool; 4] {
    let mut b = [false; 4];
    b[index] = true;
    b
}

fn controls(index: Option<usize>, input_db: f64, attack: f64, release: f64) -> Controls {
    Controls {
        input_db,
        output_db: 0.0,
        attack,
        release,
        buttons: match index {
            Some(i) => buttons(i),
            None => [true; 4],
        },
    }
}

/// Settled output level of a 1 kHz tone, in dBFS.
fn steady_db(c: Controls, input_db: f64) -> f64 {
    let mut ch = Channel::new(REV_D, FS, 4, 1);
    ch.set_controls(c);
    let amp = 10f64.powf(input_db / 20.0);
    let w = std::f64::consts::TAU * 1000.0 / FS;
    for n in 0..(FS as usize * 3) {
        ch.process((amp * (w * n as f64).sin()) as f32);
    }
    let win = FS as usize;
    let (mut re, mut im) = (0.0, 0.0);
    for n in 0..win {
        let p = w * n as f64;
        let y = ch.process((amp * p.sin()) as f32) as f64;
        re += y * p.sin();
        im += y * p.cos();
    }
    20.0 * (2.0 * (re * re + im * im).sqrt() / win as f64 + 1e-15).log10()
}

/// Gain reduction against time for a tone that switches on at t=0.
/// Returns (time to 63 % of final GR, final GR).
fn attack_time(c: Controls, input_db: f64) -> (f64, f64) {
    let mut ch = Channel::new(REV_D, FS, 4, 1);
    ch.set_controls(c);
    let amp = 10f64.powf(input_db / 20.0);
    let w = std::f64::consts::TAU * 1000.0 / FS;
    // Settle at silence first.
    for _ in 0..(FS as usize / 10) {
        ch.process(0.0);
    }
    let n_max = (FS * 0.5) as usize;
    let mut gr = Vec::with_capacity(n_max);
    for n in 0..n_max {
        ch.process((amp * (w * n as f64).sin()) as f32);
        gr.push(ch.gain_reduction_db());
    }
    let final_gr = gr[n_max - 1];
    let target = final_gr * 0.63;
    let idx = gr.iter().position(|&g| g >= target).unwrap_or(n_max - 1);
    (idx as f64 / FS, final_gr)
}

/// Time to recover 63 % of the way back after the tone stops.
fn release_time(c: Controls, input_db: f64) -> f64 {
    let mut ch = Channel::new(REV_D, FS, 4, 1);
    ch.set_controls(c);
    let amp = 10f64.powf(input_db / 20.0);
    let w = std::f64::consts::TAU * 1000.0 / FS;
    for n in 0..(FS as usize * 2) {
        ch.process((amp * (w * n as f64).sin()) as f32);
    }
    let start = ch.gain_reduction_db();
    let target = start * 0.37; // 63 % recovered
    let n_max = (FS * 4.0) as usize;
    for n in 0..n_max {
        ch.process(0.0);
        if ch.gain_reduction_db() <= target {
            return n as f64 / FS;
        }
    }
    f64::INFINITY
}

/// Total harmonic distortion of a 1 kHz tone, in percent.
fn thd_percent(c: Controls, input_db: f64) -> f64 {
    let mut ch = Channel::new(REV_D, FS, 4, 1);
    ch.set_controls(c);
    let amp = 10f64.powf(input_db / 20.0);
    let w = std::f64::consts::TAU * 1000.0 / FS;
    for n in 0..(FS as usize * 2) {
        ch.process((amp * (w * n as f64).sin()) as f32);
    }
    let win = FS as usize;
    let mut y = Vec::with_capacity(win);
    for n in 0..win {
        y.push(ch.process((amp * (w * n as f64).sin()) as f32) as f64);
    }
    let bin = |h: f64| {
        let (mut re, mut im) = (0.0, 0.0);
        for (n, &v) in y.iter().enumerate() {
            let p = w * h * n as f64;
            re += v * p.sin();
            im += v * p.cos();
        }
        2.0 * (re * re + im * im).sqrt() / win as f64
    };
    let f1 = bin(1.0);
    let harm: f64 = (2..=8).map(|h| bin(h as f64).powi(2)).sum::<f64>().sqrt();
    100.0 * harm / f1
}

/// The detector's own time constant, measured open loop: hold the input it
/// sees constant and watch the envelope rise. This is what the 20 us / 800 us
/// and 50 ms / 1100 ms figures on the data sheet describe -- the attack and
/// release networks -- rather than anything about a tone burst.
fn detector_times(knob_attack: f64, knob_release: f64) -> (f64, f64) {
    let fs = 768_000.0;
    let mut d = Detector::new(fs);
    d.set_timing(Timing {
        k: 3.0,
        attack: detector::knob_to_time(knob_attack, detector::ATTACK_FASTEST, detector::ATTACK_SLOWEST),
        release: detector::knob_to_time(knob_release, detector::RELEASE_FASTEST, detector::RELEASE_SLOWEST),
        knee: 0.0,
    });
    // A steady level well above the operating point.
    let level = 10f64.powf(-6.0 / 20.0);
    let settled = {
        let mut last = 0.0;
        for _ in 0..(fs as usize * 4) {
            last = d.process(level);
        }
        last
    };
    d.reset();
    let mut attack = f64::NAN;
    for n in 0..(fs as usize) {
        if d.process(level) >= settled * 0.63 {
            attack = n as f64 / fs;
            break;
        }
    }
    for _ in 0..(fs as usize * 4) {
        d.process(level);
    }
    let mut release = f64::NAN;
    for n in 0..(fs as usize * 8) {
        if d.process(0.0) <= settled * 0.37 {
            release = n as f64 / fs;
            break;
        }
    }
    (attack, release)
}

fn main() {
    if std::env::args().any(|a| a == "--spec") {
        // The published figures for the 1176LN, each measured the way the
        // line is worded.
        let mut fails = 0;
        let mut check = |name: &str, got: String, ok: bool| {
            println!("  {:<44} {:>18}   {}", name, got, if ok { "ok" } else { "OFF SPEC" });
            if !ok { fails += 1; }
        };

        // -- frequency response, unity gain, no compression ----------------
        let flat = Controls { input_db: 0.0, output_db: 0.0, attack: 0.5, release: 0.5, buttons: [false; 4] };
        let response_at = |hz: f64, fs: f64, os: usize| -> f64 {
            let mut ch = Channel::new(REV_D, fs, os, 1);
            ch.set_controls(flat);
            let amp = 10f64.powf(-24.0 / 20.0);
            let w = std::f64::consts::TAU * hz / fs;
            for n in 0..(fs as usize / 2) { ch.process((amp * (w * n as f64).sin()) as f32); }
            let win = (fs as usize).min((fs / hz * 200.0) as usize);
            let (mut re, mut im) = (0.0, 0.0);
            for n in 0..win {
                let p = w * n as f64;
                let y = ch.process((amp * p.sin()) as f32) as f64;
                re += y * p.sin(); im += y * p.cos();
            }
            20.0 * ((2.0 * (re * re + im * im).sqrt() / win as f64) / amp).log10()
        };
        for fs in [44_100.0, 48_000.0] {
        println!("== response at a {fs:.0} Hz session, referred to 1 kHz ==");
        for os in [1usize, 2, 4] {
            let mid = response_at(1000.0, fs, os);
            let label = if os == 1 { String::from("off") } else { format!("{os}x") };
            print!("  oversampling {label:<4}");
            for hz in [10_000.0, 15_000.0, 18_000.0, 20_000.0] {
                print!("   {hz:.0} Hz {:+6.2} dB", response_at(hz, fs, os) - mid);
            }
            println!();
        }
        println!();
        }
        let response = |hz: f64| -> f64 {
            let mut ch = Channel::new(REV_D, FS, 4, 1);
            ch.set_controls(flat);
            let amp = 10f64.powf(-24.0 / 20.0);
            let w = std::f64::consts::TAU * hz / FS;
            for n in 0..(FS as usize / 2) { ch.process((amp * (w * n as f64).sin()) as f32); }
            let win = (FS as usize).min((FS / hz * 200.0) as usize);
            let (mut re, mut im) = (0.0, 0.0);
            for n in 0..win {
                let p = w * n as f64;
                let y = ch.process((amp * p.sin()) as f32) as f64;
                re += y * p.sin(); im += y * p.cos();
            }
            let mag = 2.0 * (re * re + im * im).sqrt() / win as f64;
            20.0 * (mag / amp).log10()
        };
        let mid = response(1000.0);
        let mut worst = 0.0f64;
        let mut worst_hz = 0.0;
        println!("== frequency response, referred to 1 kHz ==");
        for hz in [20.0, 30.0, 50.0, 100.0, 1000.0, 10_000.0, 16_000.0, 20_000.0] {
            let d = response(hz) - mid;
            println!("  {hz:>8.0} Hz  {d:+6.2} dB");
            if d.abs() > worst.abs() { worst = d; worst_hz = hz; }
        }
        println!("\n== published specification ==");
        check("frequency response, 20 Hz to 20 kHz, +/-1 dB",
              format!("{worst:+.2} dB at {worst_hz:.0} Hz"), worst.abs() <= 1.0);

        // -- ratios ---------------------------------------------------------
        for (i, marked) in [4.0, 8.0, 12.0, 20.0].iter().enumerate() {
            let c = controls(Some(i), 20.0, 0.5, 0.5);
            let lo = steady_db(c, -30.0);
            let hi = steady_db(c, -20.0);
            let got = 10.0 / (hi - lo);
            check(&format!("ratio {marked:.0}:1"), format!("{got:.3}:1"),
                  (got - marked).abs() / marked <= 0.02);
        }

        // -- timing ----------------------------------------------------------
        for (label, knob, want, fastest) in [
            ("attack, fully clockwise", 1.0, 20e-6, true),
            ("attack, fully anticlockwise", 0.0, 800e-6, false),
        ] {
            let _ = fastest;
            let set = detector::knob_to_time(knob, detector::ATTACK_FASTEST, detector::ATTACK_SLOWEST);
            let (a, _) = detector_times(knob, 0.5);
            check(&format!("{label} ({:.0} us)", want * 1e6),
                  format!("{:.1} us", a * 1e6), (a - set).abs() / set < 0.05 && (set - want).abs() / want < 1e-6);
        }
        for (label, knob, want) in [
            ("release, fully clockwise", 1.0, 50e-3),
            ("release, fully anticlockwise", 0.0, 1.1),
        ] {
            let set = detector::knob_to_time(knob, detector::RELEASE_FASTEST, detector::RELEASE_SLOWEST);
            let (_, r) = detector_times(0.5, knob);
            check(&format!("{label} ({:.0} ms)", want * 1e3),
                  format!("{:.1} ms", r * 1e3), (r - set).abs() / set < 0.05 && (set - want).abs() / want < 1e-6);
        }

        // -- distortion, at a nominal level with the unit idle ---------------
        for rev in [REV_A, REV_D, REV_F] {
            let t = thd_rev(rev, flat, -18.0);
            check(&format!("distortion, {} at -18 dBFS, under 0.5 %", rev.name),
                  format!("{t:.3} %"), t < 0.5);
        }

        // -- noise ------------------------------------------------------------
        for rev in [Revision { noise_floor_db: -86.0, ..REV_A },
                    Revision { noise_floor_db: -96.0, ..REV_D },
                    Revision { noise_floor_db: -98.0, ..REV_F }] {
            let mut ch = Channel::new(rev, FS, 4, 1);
            ch.set_controls(flat);
            let mut sum = 0.0;
            let n = FS as usize;
            for _ in 0..n { let y = ch.process(0.0) as f64; sum += y * y; }
            let rms = (sum / n as f64).sqrt();
            let db = 20.0 * (rms + 1e-30).log10();
            check(&format!("signal to noise, {}, better than 75 dB", rev.name),
                  format!("{:.1} dB", -db), -db > 75.0);
        }

        // -- gain reduction available -----------------------------------------
        let c = controls(Some(3), 40.0, 0.5, 0.5);
        let mut ch = Channel::new(REV_D, FS, 4, 1);
        ch.set_controls(c);
        let amp = 10f64.powf(-12.0 / 20.0);
        let w = std::f64::consts::TAU * 1000.0 / FS;
        for n in 0..(FS as usize * 3) { ch.process((amp * (w * n as f64).sin()) as f32); }
        let gr = ch.gain_reduction_db();
        check("gain reduction available, at least 40 dB", format!("{gr:.1} dB"), gr >= 40.0);

        // Where does the last percent of ratio error come from? A unit with
        // the gain element and the output stage made linear should land on the
        // marked figure exactly, since the loop's static solution is 1 + k.
        let clean = Revision { fet_drive: 0.0, amp_drive: 0.0, fet_bias: 0.0, ..REV_D };
        println!("\n== the same ratios through a linearised unit ==");
        for (i, marked) in [4.0, 8.0, 12.0, 20.0].iter().enumerate() {
            let c = controls(Some(i), 20.0, 0.5, 0.5);
            let mut lo_ch = Channel::new(clean, FS, 4, 1);
            lo_ch.set_controls(c);
            let measure = |lvl: f64| {
                let mut ch = Channel::new(clean, FS, 4, 1);
                ch.set_controls(c);
                let amp = 10f64.powf(lvl / 20.0);
                let w = std::f64::consts::TAU * 1000.0 / FS;
                for n in 0..(FS as usize * 3) { ch.process((amp * (w * n as f64).sin()) as f32); }
                let win = FS as usize;
                let (mut re, mut im) = (0.0, 0.0);
                for n in 0..win {
                    let p = w * n as f64;
                    let y = ch.process((amp * p.sin()) as f32) as f64;
                    re += y * p.sin(); im += y * p.cos();
                }
                20.0 * (2.0 * (re * re + im * im).sqrt() / win as f64 + 1e-15).log10()
            };
            let got = 10.0 / (measure(-20.0) - measure(-30.0));
            println!("  marked {marked:>5.0}:1   linearised {got:8.4}:1   error {:+.3} %",
                     (got - marked) / marked * 100.0);
        }

        println!("\n  {} line(s) off specification", fails);
        return;
    }
    if std::env::args().any(|a| a == "--presets") {
        println!("== what each built-in preset does to a -18 dBFS tone ==");
        println!("  preset                 input  output      GR     net     THD");
        for name in ["Vocal 4:1", "All Buttons In", "Bass 8:1", "Drum Buss", "Room Crush", "Parallel Smash"] {
            let Some(dials) = comp76fx_core::presets::built_in_dials(name) else { continue };
            let dial = |id: &str| dials.iter().find(|(n, _)| *n == id).map(|(_, v)| *v as f64);
            let (Some(input), Some(output)) = (dial("input"), dial("output")) else { continue };
            let c = Controls {
                input_db: input,
                output_db: output,
                attack: dial("attack").unwrap_or(4.0) / 7.0,
                release: dial("release").unwrap_or(4.0) / 7.0,
                buttons: [
                    dial("ratio4").unwrap_or(0.0) > 0.5, dial("ratio8").unwrap_or(0.0) > 0.5,
                    dial("ratio12").unwrap_or(0.0) > 0.5, dial("ratio20").unwrap_or(0.0) > 0.5,
                ],
            };
            let mut ch = Channel::new(REV_D, FS, 4, 1);
            ch.set_controls(c);
            let amp = 10f64.powf(-18.0 / 20.0);
            let w = std::f64::consts::TAU * 1000.0 / FS;
            for n in 0..(FS as usize * 3) { ch.process((amp * (w * n as f64).sin()) as f32); }
            let gr = ch.gain_reduction_db();
            let out = steady_db(c, -18.0);
            let thd = thd_rev(REV_D, c, -18.0);
            println!("  {name:<22} {input:>5.0} {output:>7.0} {gr:>7.2} {:>7.2} dB {thd:>7.2} %", out + 18.0);
        }
        return;
    }
    if std::env::args().any(|a| a == "--allbutton") {
        // The preset as it ships: input 17, output -11, both dials fully
        // clockwise, all four switches in.
        // Read from the shipped preset rather than a copy of it, so this
        // measures what people actually load.
        let dials = comp76fx_core::presets::built_in_dials("All Buttons In").unwrap();
        let dial = |id: &str| dials.iter().find(|(n, _)| *n == id).map(|(_, v)| *v as f64).unwrap();
        let preset = Controls {
            input_db: dial("input"),
            output_db: dial("output"),
            attack: dial("attack") / 7.0,
            release: dial("release") / 7.0,
            buttons: [true; 4],
        };
        let four   = Controls { buttons: [true, false, false, false], ..preset };
        println!("== the All Buttons In preset, against 4:1 at the same settings ==");
        println!("  signal      all-in GR   4:1 GR    all-in THD   4:1 THD");
        for lvl in [-30.0, -24.0, -18.0, -12.0] {
            let a_out = steady_db(preset, lvl);
            let f_out = steady_db(four, lvl);
            println!("  {lvl:>5.0} dBFS   {:>7.2} dB {:>8.2} dB   {:>8.3} % {:>8.3} %",
                lvl + 17.0 - 11.0 - a_out - 11.0 + 11.0, lvl + 17.0 - f_out - 11.0 + 11.0 - 17.0 + 17.0,
                thd_rev(REV_D, preset, lvl), thd_rev(REV_D, four, lvl));
        }
        println!("\n== gain reduction actually applied (input drive against GR) ==");
        for lvl in [-30.0, -24.0, -18.0, -12.0] {
            let mut ch = Channel::new(REV_D, FS, 4, 1);
            ch.set_controls(preset);
            let amp = 10f64.powf(lvl / 20.0);
            let w = std::f64::consts::TAU * 1000.0 / FS;
            for n in 0..(FS as usize * 3) { ch.process((amp * (w * n as f64).sin()) as f32); }
            let all_gr = ch.gain_reduction_db();
            let mut ch = Channel::new(REV_D, FS, 4, 1);
            ch.set_controls(four);
            for n in 0..(FS as usize * 3) { ch.process((amp * (w * n as f64).sin()) as f32); }
            println!("  {lvl:>5.0} dBFS   all-in {all_gr:>6.2} dB    4:1 {:>6.2} dB", ch.gain_reduction_db());
        }
        println!("\n== the knee: gain reduction as the level comes up ==");
        println!("  signal      all-in      4:1");
        for lvl in [-48.0, -44.0, -40.0, -36.0, -32.0, -28.0, -24.0] {
            let mut a = Channel::new(REV_D, FS, 4, 1); a.set_controls(preset);
            let mut f = Channel::new(REV_D, FS, 4, 1); f.set_controls(four);
            let amp = 10f64.powf(lvl / 20.0);
            let w = std::f64::consts::TAU * 1000.0 / FS;
            for n in 0..(FS as usize * 2) {
                let x = (amp * (w * n as f64).sin()) as f32;
                a.process(x); f.process(x);
            }
            println!("  {lvl:>5.0} dBFS  {:>6.2} dB  {:>6.2} dB", a.gain_reduction_db(), f.gain_reduction_db());
        }
        println!("\n== onset: gain reduction over the first 12 ms, -18 dBFS ==");
        for (label, c) in [("all buttons in", preset), ("4:1", four)] {
            let mut ch = Channel::new(REV_D, FS, 4, 1);
            ch.set_controls(c);
            let amp = 10f64.powf(-18.0 / 20.0);
            let w = std::f64::consts::TAU * 1000.0 / FS;
            for _ in 0..(FS as usize / 10) { ch.process(0.0); }
            println!("  {label}:");
            let n = (FS * 0.012) as usize;
            let step = n / 24;
            for i in 0..n {
                ch.process((amp * (w * i as f64).sin()) as f32);
                if i % step == 0 {
                    let g = ch.gain_reduction_db();
                    println!("    {:>6.2} ms  {:5.2} dB {}", i as f64 / FS * 1e3, g, "#".repeat((g * 1.5) as usize));
                }
            }
        }
        return;
    }
    if std::env::args().any(|a| a == "--thd") {
        println!("== THD vs signal level, no gain reduction (spec: under 0.5 %) ==");
        println!("  level      Rev A    Rev D    Rev F");
        for lvl in [-30.0, -24.0, -18.0, -12.0, -6.0] {
            let c = controls(None, 0.0, 0.5, 0.5);
            let c = Controls { buttons: [false; 4], ..c };
            print!("  {lvl:>5.0} dBFS");
            for rev in [REV_A, REV_D, REV_F] {
                print!("  {:>6.3} %", thd_rev(rev, c, lvl));
            }
            println!();
        }
        println!("\n== THD vs gain reduction, 4:1, signal -18 dBFS ==");
        println!("  drive      Rev A    Rev D    Rev F");
        for drive in [0.0, 6.0, 12.0, 18.0, 24.0, 30.0] {
            let c = controls(Some(0), drive, 0.5, 0.5);
            print!("  {drive:>4.0} dB  ");
            for rev in [REV_A, REV_D, REV_F] {
                print!("  {:>6.3} %", thd_rev(rev, c, -18.0));
            }
            println!();
        }
        println!("\n== all buttons in, signal -18 dBFS ==");
        for drive in [12.0, 24.0] {
            let c = controls(None, drive, 0.5, 0.5);
            print!("  drive {drive:>4.0} dB");
            for rev in [REV_A, REV_D, REV_F] {
                print!("  {:>6.3} %", thd_rev(rev, c, -18.0));
            }
            println!();
        }
        println!("\n== harmonic balance at -12 dBFS, no GR (2nd / 3rd, dB below fundamental) ==");
        for rev in [REV_A, REV_D, REV_F] {
            let c = Controls { buttons: [false; 4], ..controls(None, 0.0, 0.5, 0.5) };
            let (h2, h3) = harmonics(rev, c, -12.0);
            println!("  {:<6} 2nd {h2:>7.1} dB   3rd {h3:>7.1} dB", rev.name);
        }
        return;
    }
    if std::env::args().any(|a| a == "--times") {
        println!("== detector attack, open loop (spec 20 us fastest .. 800 us slowest) ==");
        for (label, k) in [("knob 7", 1.0), ("knob 6", 5.0/6.0), ("knob 4", 0.5), ("knob 2", 1.0/6.0), ("knob 1", 0.0)] {
            let want = detector::knob_to_time(k, detector::ATTACK_FASTEST, detector::ATTACK_SLOWEST);
            let (a, _) = detector_times(k, 0.5);
            println!("  {label}  set {:>7.1} us -> measured {:>8.1} us  ({:.2}x)", want*1e6, a*1e6, a/want);
        }
        println!("\n== detector release, open loop (spec 50 ms fastest .. 1100 ms slowest) ==");
        for (label, k) in [("knob 7", 1.0), ("knob 6", 5.0/6.0), ("knob 4", 0.5), ("knob 2", 1.0/6.0), ("knob 1", 0.0)] {
            let want = detector::knob_to_time(k, detector::RELEASE_FASTEST, detector::RELEASE_SLOWEST);
            let (_, r) = detector_times(0.5, k);
            println!("  {label}  set {:>7.1} ms -> measured {:>8.1} ms  ({:.2}x)", want*1e3, r*1e3, r/want);
        }
        return;
    }
    if std::env::args().any(|a| a == "--trace") {
        println!("== GR trace, 4:1, attack knob 7 (fastest), first 3 ms ==");
        trace(controls(Some(0), 20.0, 1.0, 0.5), -20.0, 3000.0);
        println!("\n== GR trace, 4:1, attack knob 1 (slowest), first 3 ms ==");
        trace(controls(Some(0), 20.0, 0.0, 0.5), -20.0, 3000.0);
        return;
    }
    println!("== static ratio (slope of output vs input, -20 to -10 dBFS in) ==");
    for (name, idx) in [("4:1", Some(0)), ("8:1", Some(1)), ("12:1", Some(2)), ("20:1", Some(3)), ("all", None)] {
        let c = controls(idx, 20.0, 0.5, 0.5);
        let lo = steady_db(c, -30.0);
        let hi = steady_db(c, -20.0);
        println!("  {name:>4}  marked -> measured {:5.2}:1", 10.0 / (hi - lo));
    }

    println!("\n== attack time to 63 % of final GR (spec: 20 us fastest, 800 us slowest) ==");
    for (label, knob) in [("knob 7 (fastest)", 1.0), ("knob 4", 0.5), ("knob 1 (slowest)", 0.0)] {
        let c = controls(Some(0), 20.0, knob, 0.5);
        let (t, gr) = attack_time(c, -20.0);
        println!("  {label:<18} {:>9.1} us   (final GR {:.1} dB)", t * 1e6, gr);
    }

    println!("\n== release time to 63 % recovery (spec: 50 ms fastest, 1100 ms slowest) ==");
    for (label, knob) in [("knob 7 (fastest)", 1.0), ("knob 4", 0.5), ("knob 1 (slowest)", 0.0)] {
        let c = controls(Some(0), 20.0, 0.5, knob);
        println!("  {label:<18} {:>9.1} ms", release_time(c, -20.0) * 1e3);
    }

    println!("\n== gain reduction against input drive, 4:1 (knee) ==");
    for drive in [0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0] {
        let c = controls(Some(0), drive, 0.5, 0.5);
        let out = steady_db(c, -30.0);
        println!("  input {drive:>4.0} dB -> out {out:7.2} dBFS   GR {:5.2} dB", -30.0 + drive - out);
    }

    println!("\n== THD at 1 kHz (spec: under 0.5 %) ==");
    for (label, drive) in [("no GR", 0.0), ("~4 dB GR", 12.0), ("~10 dB GR", 20.0), ("~20 dB GR", 30.0)] {
        let c = controls(Some(0), drive, 0.5, 0.5);
        println!("  {label:<10} {:>6.3} %", thd_percent(c, -30.0));
    }
}

/// Prints the gain reduction trace after a tone switches on, so the shape of
/// the attack can be looked at rather than inferred from one crossing.
#[allow(dead_code)]
pub fn trace(c: Controls, input_db: f64, micros: f64) {
    let mut ch = Channel::new(REV_D, FS, 4, 1);
    ch.set_controls(c);
    let amp = 10f64.powf(input_db / 20.0);
    let w = std::f64::consts::TAU * 1000.0 / FS;
    for _ in 0..(FS as usize / 10) {
        ch.process(0.0);
    }
    let n = (FS * micros * 1e-6) as usize;
    let step = (n / 40).max(1);
    for i in 0..n {
        ch.process((amp * (w * i as f64).sin()) as f32);
        if i % step == 0 {
            let g = ch.gain_reduction_db();
            println!("    {:>8.1} us  {:6.2} dB  {}", i as f64 / FS * 1e6, g, "#".repeat((g * 2.0) as usize));
        }
    }
}
