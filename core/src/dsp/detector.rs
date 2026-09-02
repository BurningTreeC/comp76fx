//! The sidechain.
//!
//! The 1176 is a feedback compressor: the detector samples the signal *after*
//! the gain element, so the loop settles rather than being computed from the
//! input. That is what gives it a naturally soft knee, and it is why the ratio
//! that comes out is set by the loop gain rather than by a curve.
//!
//! With `k` as the sidechain's gain, the static solution of the loop is
//!
//! ```text
//!     g = -k / (1 + k) * (input - threshold)
//! ```
//!
//! so the slope is `1 / (1 + k)` and the ratio is simply `1 + k`. A 4:1 button
//! is `k = 3`, 20:1 is `k = 19`.
//!
//! The timing is a rectifier charging a capacitor through the attack network
//! and discharging it through the release network. The release is not one
//! exponential: a second, slower stage runs alongside the first, which is what
//! makes the recovery program dependent rather than a fixed curve.

/// Attack times the front panel sweeps between, in seconds. Fully clockwise is
/// fastest, which is backwards from most compressors.
pub const ATTACK_FASTEST: f64 = 20e-6;
pub const ATTACK_SLOWEST: f64 = 800e-6;

/// Release times, in seconds. Also fastest fully clockwise.
pub const RELEASE_FASTEST: f64 = 50e-3;
pub const RELEASE_SLOWEST: f64 = 1.1;

/// The fixed operating point the input knob drives the signal against. The
/// hardware has no threshold control; you set how hard you hit this instead.
pub const THRESHOLD_DB: f64 = -24.0;

/// Where the sidechain amplifier begins to run out of rail, and the most it
/// can ask for however hard it is driven. Both in dB of gain reduction.
///
/// Some bound is needed or the loop chases an enormous target on a transient
/// and every attack setting collapses to the same time. It must not be a
/// plain `tanh` from zero, though: at equilibrium the demand and the gain
/// reduction are the same number, so a curve that bends from the origin bends
/// the static ratio with it and every button reads low -- which is exactly
/// what it did, by 5 % at 4:1 and 14 % at 20:1. Staying linear across the
/// range the unit actually works in keeps the ratio exact, and the knee comes
/// in only where the control voltage really would be running out.
const DEMAND_LINEAR_DB: f64 = 40.0;
const MAX_DEMAND_DB: f64 = 64.0;

/// How much of the recovery comes from the slower of the two stages.
const SLOW_STAGE_SHARE: f64 = 0.35;
/// The slow stage runs this many times longer than the release setting.
const SLOW_STAGE_RATIO: f64 = 6.0;
/// Two stages running together recover more slowly than either alone, so the
/// pair reaches the 63 % point later than the marked time unless the fast one
/// is shortened to compensate. Solving
///
/// ```text
///     (1 - share) e^-u + share e^(-u / ratio) = 0.37
/// ```
///
/// for the values above gives u = 1.7785, so without this every release
/// setting ran 78 % long. `release_compensation_is_solved` in the tests keeps
/// the two in step if the shape is ever retuned.
const RELEASE_COMPENSATION: f64 = 1.0 / 1.778_477;

#[derive(Clone, Copy, PartialEq)]
pub struct Timing {
    /// Sidechain gain. The ratio is `1 + k`.
    pub k: f64,
    pub attack: f64,
    pub release: f64,
    /// Where the sidechain starts working, in dBFS. Fixed on the hardware,
    /// except that all-button mode moves it: four ratio resistors in parallel
    /// is more sidechain gain than any one of them, and the bias shift has the
    /// FET part way on before the signal arrives.
    pub threshold: f64,
    /// Width of the knee, in dB. Zero everywhere except all-button mode,
    /// where the shifted bias makes the sidechain come on gradually instead
    /// of at a definite point, which is what lets the leading edge of a
    /// transient through before the gain collapses behind it.
    pub knee: f64,
}

pub struct Detector {
    sample_rate: f64,
    timing: Timing,
    /// Charge and discharge coefficients for the two recovery stages.
    attack_coef: f64,
    release_fast: f64,
    release_slow: f64,
    /// Envelope of each stage, in dB of gain reduction.
    fast: f64,
    slow: f64,
}

impl Detector {
    pub fn new(sample_rate: f64) -> Self {
        let mut detector = Self {
            sample_rate,
            timing: Timing {
                k: 3.0,
                attack: ATTACK_FASTEST,
                release: RELEASE_FASTEST,
                threshold: THRESHOLD_DB,
                knee: 0.0,
            },
            attack_coef: 0.0,
            release_fast: 0.0,
            release_slow: 0.0,
            fast: 0.0,
            slow: 0.0,
        };
        detector.recompute();
        detector
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.recompute();
        self.reset();
    }

    pub fn set_timing(&mut self, timing: Timing) {
        if timing == self.timing {
            return;
        }
        self.timing = timing;
        self.recompute();
    }

    fn recompute(&mut self) {
        self.attack_coef = coefficient(self.timing.attack, self.sample_rate);
        let fast = self.timing.release * RELEASE_COMPENSATION;
        self.release_fast = coefficient(fast, self.sample_rate);
        self.release_slow = coefficient(fast * SLOW_STAGE_RATIO, self.sample_rate);
    }

    /// Feed the detector the compressed output and get back the gain reduction
    /// it asks for, in dB.
    #[inline]
    pub fn process(&mut self, output: f64) -> f64 {
        // The rectifier only sees how far the output sits above the operating
        // point; below it the sidechain does nothing at all.
        let level_db = 20.0 * (output.abs() + 1e-12).log10();
        // Just the sidechain's own gain. Dividing by `1 + k` here as well
        // would apply the ratio twice, since closing the loop around the gain
        // element is what produces that term.
        let over = knee(level_db - self.timing.threshold, self.timing.knee);
        let demand = limit_demand(over * self.timing.k);

        // Charging is one time constant, recovery is two running together.
        self.fast = follow(self.fast, demand, self.attack_coef, self.release_fast);
        self.slow = follow(self.slow, demand, self.attack_coef, self.release_slow);
        self.fast * (1.0 - SLOW_STAGE_SHARE) + self.slow * SLOW_STAGE_SHARE
    }

    pub fn reset(&mut self) {
        self.fast = 0.0;
        self.slow = 0.0;
    }
}

/// How far above the operating point the rectifier sees, softened over a knee.
///
/// A hard corner would be a definite point at which the unit starts working.
/// Widening it is what all-button mode does: the sidechain comes on gradually,
/// so the front of a transient is through before there is much gain reduction
/// behind it. It has to be done here, as a curve, and not by delaying the
/// control voltage: a transport delay inside a loop with this much gain does
/// not lag, it oscillates, and it drove the demand into the rail at 64 dB
/// before any feedback arrived.
#[inline]
pub fn knee(over: f64, width: f64) -> f64 {
    if width <= 0.0 {
        return over.max(0.0);
    }
    let half = width * 0.5;
    if over <= -half {
        0.0
    } else if over >= half {
        over
    } else {
        (over + half) * (over + half) / (2.0 * width)
    }
}

/// Bends the demand over where the sidechain runs out of rail. Linear below
/// the knee, so the ratio the buttons mark is the ratio the loop settles at.
#[inline]
pub fn limit_demand(raw: f64) -> f64 {
    if raw <= DEMAND_LINEAR_DB {
        return raw;
    }
    let span = MAX_DEMAND_DB - DEMAND_LINEAR_DB;
    DEMAND_LINEAR_DB + span * ((raw - DEMAND_LINEAR_DB) / span).tanh()
}

/// Exposed so a test can check the compensation above still solves the shape.
pub const RELEASE_SHAPE: (f64, f64, f64) =
    (SLOW_STAGE_SHARE, SLOW_STAGE_RATIO, RELEASE_COMPENSATION);

/// A one pole coefficient for a time constant, taken as the usual 63 % point.
fn coefficient(seconds: f64, sample_rate: f64) -> f64 {
    if seconds <= 0.0 {
        return 0.0;
    }
    (-1.0 / (seconds * sample_rate)).exp()
}

/// Rises towards a demand at the attack rate and falls back at the release
/// rate, which is what a capacitor charged through one network and discharged
/// through another does.
#[inline]
fn follow(current: f64, demand: f64, attack: f64, release: f64) -> f64 {
    let coefficient = if demand > current { attack } else { release };
    demand + (current - demand) * coefficient
}

/// Maps a knob at `0.0..=1.0` onto a time, fully clockwise being fastest.
pub fn knob_to_time(position: f64, fastest: f64, slowest: f64) -> f64 {
    let position = position.clamp(0.0, 1.0);
    // Times of this range are heard logarithmically, so sweep them that way.
    slowest * (fastest / slowest).powf(position)
}
