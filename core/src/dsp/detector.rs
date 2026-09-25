//! The sidechain.
//!
//! The 1176 is a feedback compressor: the detector samples the signal *after*
//! the gain element, so the loop settles rather than being computed from the
//! input. That is why the ratio that comes out is set by the loop gain rather
//! than by a curve, and why the buttons land on their markings.
//!
//! It does not, on its own, soften the knee. A loop closed around a rectifier
//! with a definite threshold settles on a static curve with the same corner.
//! The softness is in the rectifier: its diodes are biased to make the
//! threshold, and a diode turns on over a span of voltage rather than at a
//! point. That span is the same for every button, so it is widest in decibels
//! where the signal at the diodes is smallest, which is the 4:1 -- the one
//! ratio the manual says has a soft knee. See [`super::diode_knee_db`].
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
//!
//! The loop is solved within each sample rather than one sample behind
//! itself. See [`Detector::process_in_loop`].

/// Attack times the front panel sweeps between, in seconds. Fully clockwise is
/// fastest, which is backwards from most compressors.
pub const ATTACK_FASTEST: f64 = 20e-6;
pub const ATTACK_SLOWEST: f64 = 800e-6;

/// Release times, in seconds. Also fastest fully clockwise.
pub const RELEASE_FASTEST: f64 = 50e-3;
pub const RELEASE_SLOWEST: f64 = 1.1;

/// The fixed operating point the input knob drives the signal against, for
/// the 20:1 button; the others sit a little below it, see
/// [`super::THRESHOLD_OFFSETS_DB`]. The hardware has no threshold control;
/// you set how hard you hit this instead.
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

/// How closely the loop is solved, in dB of demand, and how many steps that
/// is allowed to take. Newton's method on a curve that is piecewise linear
/// almost everywhere lands in one or two; the rest is a bracketed fallback.
const SOLVE_TOLERANCE_DB: f64 = 1e-9;
const SOLVE_STEPS: usize = 32;

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
/// setting ran 78 % long. `release_compensation_is_solved` in
/// `tests/calibration.rs` keeps the two in step if the shape is ever retuned.
const RELEASE_COMPENSATION: f64 = 1.0 / 1.778_477;

#[derive(Clone, Copy, PartialEq)]
pub struct Timing {
    /// Sidechain gain. The ratio is `1 + k`.
    pub k: f64,
    pub attack: f64,
    pub release: f64,
    /// Where the sidechain starts working, in dBFS. Set by the ratio button,
    /// and raised by a combination of them, which shorts part of the signal
    /// ladder and so passes the rectifier less signal.
    pub threshold: f64,
    /// Width of the knee, in dB. The rectifier diodes give every button one,
    /// narrow at 20:1 and several decibels wide at 4:1.
    pub knee: f64,
    /// How far below the point where the gain element starts to conduct the
    /// gate is resting, in dB of control. Zero with one button in. A
    /// combination pulls the gate's bias down, and the sidechain then has to
    /// charge the envelope through this much before any gain reduction
    /// happens: the front of a transient is through before the gain moves,
    /// the release reaches no reduction sooner, and the level it takes to
    /// start compressing rises.
    pub dead_zone: f64,
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
                dead_zone: 0.0,
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

    /// The attack and release networks on their own, driven by a level that
    /// takes no notice of what they do. This is what the times on the panel
    /// describe, and what the calibration tests measure; the plugin runs
    /// [`process_in_loop`](Self::process_in_loop).
    #[inline]
    pub fn process(&mut self, output: f64) -> f64 {
        let (demand, _) = self.demand(level_db(output));
        self.charge(demand)
    }

    /// Feed the detector the output the gain element made with the reduction
    /// this detector last asked for, and get back the reduction for the next
    /// sample, in dB.
    ///
    /// The loop is solved rather than run one sample behind itself. Running it
    /// behind -- the demand worked out from an output that has not yet been
    /// reduced by the charge it is about to cause -- is a step the size of the
    /// open loop gain, and a fast attack takes most of it at once. At the
    /// fastest setting without oversampling that overshot the settling point
    /// by 20 dB on the first sample of a transient, and because the envelope
    /// was then above its demand it held the excess for the whole release.
    ///
    /// So the demand is found together with the envelope it produces. The
    /// gain element is a divider, so in decibels a change in reduction is a
    /// plain subtraction from the output: had the envelope landed on `r`, the
    /// rectifier would have seen `unreduced - r`. Where the charge lands rises
    /// with the demand and the demand falls as the charge rises, so there is
    /// exactly one point where they agree, and that is the sample's demand.
    /// At equilibrium it is the same point the old loop settled on, so the
    /// ratios are unchanged; it just no longer overshoots getting there.
    #[inline]
    pub fn process_in_loop(&mut self, output: f64) -> f64 {
        let unreduced = level_db(output) + self.reduction();
        let demand = self.solve(unreduced);
        self.charge(demand)
    }

    /// The gain reduction the detector is asking for now, in dB: whatever
    /// of the envelope has got past the dead zone.
    pub fn reduction(&self) -> f64 {
        (self.envelope() - self.timing.dead_zone).max(0.0)
    }

    /// The control voltage as the gain reduction meter reads it, in dB. The
    /// meter measures the gate's bias against the rest point it was
    /// calibrated at, so with the gate pulled below that point it reads
    /// less than no reduction -- the needle rests past zero -- until the
    /// envelope has charged through the dead zone, and then reads true.
    pub fn control_db(&self) -> f64 {
        self.envelope() - self.timing.dead_zone
    }

    /// The charge on the envelope, from the gate's actual resting point.
    fn envelope(&self) -> f64 {
        self.fast * (1.0 - SLOW_STAGE_SHARE) + self.slow * SLOW_STAGE_SHARE
    }

    /// The reduction an envelope of `envelope` makes, and how fast that moves
    /// with it.
    #[inline]
    fn past_dead_zone(&self, envelope: f64) -> (f64, f64) {
        let past = envelope - self.timing.dead_zone;
        if past > 0.0 {
            (past, 1.0)
        } else {
            (0.0, 0.0)
        }
    }

    /// Charging is one time constant, recovery is two running together.
    #[inline]
    fn charge(&mut self, demand: f64) -> f64 {
        self.fast = follow(self.fast, demand, self.attack_coef, self.release_fast);
        self.slow = follow(self.slow, demand, self.attack_coef, self.release_slow);
        self.reduction()
    }

    /// What the sidechain asks for at a level at the rectifier, and how fast
    /// that changes with the level.
    #[inline]
    fn demand(&self, level_db: f64) -> (f64, f64) {
        // The rectifier only sees how far the output sits above the operating
        // point; below it the sidechain does nothing at all. The gain is just
        // the sidechain's own: dividing by `1 + k` here as well would apply
        // the ratio twice, since closing the loop is what produces that term.
        let (over, d_over) = knee_with_slope(level_db - self.timing.threshold, self.timing.knee);
        // The rail is an absolute limit on the control voltage. A gate
        // resting lower has that much further to go to reach it, so the
        // demand the envelope can be driven to rises with the dead zone and
        // the most reduction available does not change.
        let (demand, d_demand) = limit_with_slope(over * self.timing.k, self.timing.dead_zone);
        (demand, d_demand * self.timing.k * d_over)
    }

    /// Where the envelope lands this sample if the demand is `demand`, and
    /// how fast that moves with the demand. Each stage charges or discharges
    /// depending on which side of the demand it is, exactly as `charge` will.
    #[inline]
    fn landing(&self, demand: f64) -> (f64, f64) {
        let stage = |current: f64, release: f64| {
            let coef = if demand > current {
                self.attack_coef
            } else {
                release
            };
            (demand + (current - demand) * coef, 1.0 - coef)
        };
        let (fast, d_fast) = stage(self.fast, self.release_fast);
        let (slow, d_slow) = stage(self.slow, self.release_slow);
        (
            fast * (1.0 - SLOW_STAGE_SHARE) + slow * SLOW_STAGE_SHARE,
            d_fast * (1.0 - SLOW_STAGE_SHARE) + d_slow * SLOW_STAGE_SHARE,
        )
    }

    /// The demand that agrees with the reduction it causes.
    ///
    /// `unreduced` is the level the rectifier would see with no reduction.
    /// The root of `h(d) = d - demand(unreduced - reduction(landing(d)))` is found by
    /// Newton's method inside a bracket that always holds it: `h` rises with
    /// `d`, is below zero at nothing, and is at or above zero at the demand
    /// the envelope would meet if it released as far as it can this sample.
    #[inline]
    fn solve(&self, unreduced: f64) -> f64 {
        let (floor, _) = self.landing(0.0);
        let (ceiling, _) = self.demand(unreduced - self.past_dead_zone(floor).0);
        // Still under the operating point after releasing: nothing to solve,
        // which is also every sample below threshold and near a zero crossing.
        if ceiling <= 0.0 {
            return 0.0;
        }

        let (mut low, mut high) = (0.0, ceiling);
        let mut demand = ceiling;
        for _ in 0..SOLVE_STEPS {
            let (landed, d_landed) = self.landing(demand);
            let (reduced, d_reduced) = self.past_dead_zone(landed);
            let (wanted, d_wanted) = self.demand(unreduced - reduced);
            let error = demand - wanted;
            if error.abs() <= SOLVE_TOLERANCE_DB {
                break;
            }
            if error > 0.0 {
                high = demand;
            } else {
                low = demand;
            }
            // The slope is at least one, so the step is always defined.
            let next = demand - error / (1.0 + d_wanted * d_reduced * d_landed);
            demand = if next > low && next < high {
                next
            } else {
                0.5 * (low + high)
            };
        }
        demand
    }

    pub fn reset(&mut self) {
        self.fast = 0.0;
        self.slow = 0.0;
    }
}

/// How far above the operating point the rectifier sees, softened over a knee.
///
/// A hard corner would be a definite point at which the unit starts working.
/// The rectifier diodes round it off a little at every button and a good deal
/// at 4:1. Widening it much further is what all-button mode does: the
/// sidechain comes on gradually,
/// so the front of a transient is through before there is much gain reduction
/// behind it. It has to be done here, as a curve, and not by delaying the
/// control voltage: a transport delay inside a loop with this much gain does
/// not lag, it oscillates, and it drove the demand into the rail at 64 dB
/// before any feedback arrived.
#[inline]
pub fn knee(over: f64, width: f64) -> f64 {
    knee_with_slope(over, width).0
}

/// [`knee`] and its derivative, which the loop solver steps along.
#[inline]
fn knee_with_slope(over: f64, width: f64) -> (f64, f64) {
    if width <= 0.0 {
        return if over > 0.0 { (over, 1.0) } else { (0.0, 0.0) };
    }
    let half = width * 0.5;
    if over <= -half {
        (0.0, 0.0)
    } else if over >= half {
        (over, 1.0)
    } else {
        let rise = over + half;
        (rise * rise / (2.0 * width), rise / width)
    }
}

/// Bends the demand over where the sidechain runs out of rail. Linear below
/// the knee, so the ratio the buttons mark is the ratio the loop settles at.
#[inline]
pub fn limit_demand(raw: f64) -> f64 {
    limit_with_slope(raw, 0.0).0
}

/// [`limit_demand`] and its derivative. `lift` raises the whole curve, rail
/// and all, for a gate resting lower.
#[inline]
fn limit_with_slope(raw: f64, lift: f64) -> (f64, f64) {
    let linear = DEMAND_LINEAR_DB + lift;
    if raw <= linear {
        return (raw, 1.0);
    }
    let span = MAX_DEMAND_DB - DEMAND_LINEAR_DB;
    let bend = ((raw - linear) / span).tanh();
    (linear + span * bend, 1.0 - bend * bend)
}

/// A sample's level in dB. The offset keeps silence finite.
#[inline]
fn level_db(sample: f64) -> f64 {
    20.0 * (sample.abs() + 1e-12).log10()
}

/// Exposed so `release_compensation_is_solved` can check the compensation
/// above still solves the shape.
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
