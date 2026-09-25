//! The amplifier stages and the iron around them.
//!
//! The revisions differ here more than anywhere else. The early units run a
//! Class A output, which has no crossover region at all and saturates
//! gradually and asymmetrically. Later ones use a push-pull Class AB stage,
//! which is cleaner and more symmetrical but has a crossover region of its own.
//! Both sit behind a transformer, which rounds the top and softens the bottom.
//!
//! Ahead of the output stage are the signal preamplifier and the input of the
//! line amplifier, and there the Rev A is the odd one out: it is the only
//! revision whose amplifiers start with a junction FET rather than a bipolar
//! transistor (see [`Transistors`]).

/// A one pole filter for the band limits of the transformers, and for the
/// coupling capacitor in front of the sidechain.
#[derive(Default, Clone, Copy)]
pub(crate) struct OnePole {
    a: f64,
    z: f64,
}

impl OnePole {
    pub(crate) fn set_cutoff(&mut self, freq: f64, sample_rate: f64) {
        // Only the bottom is clamped. Holding the corner below Nyquist looks
        // like caution but does the opposite: at 44.1 kHz it dragged the
        // transformer's 55 kHz corner down to 19.8 kHz, turning a gentle tilt
        // into a wall inside the audio band. The coefficient needs no such
        // help -- as the corner rises it goes to zero on its own, which is a
        // pass through, and a pole that far outside the band is worth well
        // under a decibel in it anyway. Oversampling raises the internal rate
        // and the pole is represented properly again.
        let f = freq.max(1.0);
        self.a = (-std::f64::consts::TAU * f / sample_rate).exp();
    }

    #[inline]
    fn lowpass(&mut self, x: f64) -> f64 {
        self.z = x * (1.0 - self.a) + self.z * self.a;
        self.z
    }

    #[inline]
    pub(crate) fn highpass(&mut self, x: f64) -> f64 {
        x - self.lowpass(x)
    }

    pub(crate) fn reset(&mut self) {
        self.z = 0.0;
    }
}

/// What a revision's signal preamplifier and line amplifier are built on.
///
/// Both are feedback amplifiers of two or three transistors, and the first
/// device of each is the one this names. The schematics in the UREI manual
/// show which: the Rev A's preamplifier starts with a JFET (Q2) into a
/// 2N3707, and its line amplifier with another (Q4) into a 2N3707 and the
/// output transistor; the Rev D's and the Rev F's use 2N3391A bipolars in
/// the same places, the two preamplifiers identical down to their values.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Transistors {
    /// Bipolar throughout, from the Rev C on. The manual's description of
    /// the preamplifier is that "a large amount of overall negative feedback
    /// results in low distortion", so it passes the signal unchanged here,
    /// and the line amplifier's colour is its output stage's.
    Bipolar,
    /// A junction FET at the input of each: the Rev A. See [`SquareLaw`].
    Jfet,
}

/// The second order coefficient of a JFET-input amplifier's transfer, as it
/// comes out once the amplifier's feedback has done what it can: the output
/// is `x - JFET_CURVE * x^2` for a small signal `x`.
///
/// Nothing measured gives it. The manual documents the Rev A's circuit but
/// not its distortion, and the revision history says only that the Rev A
/// "imparts more harmonic distortion". Before these stages were modelled the
/// Rev A had its output stage run harder than the Rev D's to say so, which
/// the schematics do not support -- the output transistor, its transformer
/// and the resistors around them are the same parts on both. So its output
/// stage is now driven as the D's is, and this is sized to put back what that
/// took away: the two stages together restore the Rev A's measured
/// distortion with no gain reduction, 0.48 % at -18 dBFS, where the D reads
/// 0.33 %. What changes is where it comes from -- second harmonic added
/// ahead of the output control as well as after it, and none of the third
/// that driving the output stage harder added.
const JFET_CURVE: f64 = 0.0120;

/// The level at which a JFET-input amplifier's first device cuts off, at the
/// amplifier's output.
///
/// A JFET conducts across the whole of one half of its swing and turns off
/// on the other, where the amplifier then runs out of drive however hard the
/// feedback asks. The amplifiers' headroom is not documented either; this is
/// +6 dBFS, above anything the unit is fed at when it is limiting, and near
/// where the output stage itself starts to saturate.
const JFET_CUTOFF: f64 = 2.0;

/// A feedback amplifier whose first device is a junction FET.
///
/// In saturation a JFET's drain current is the square of its gate voltage
/// above pinch-off, so the stage's transconductance grows linearly with the
/// signal on it. Written for the error signal `e` at the gate, as a voltage
/// referred back to it, and `v` the gate's resting overdrive,
///
/// ```text
///     f(e) = e + e^2 / 2v,    e > -v
///     f(e) = -v / 2,          e <= -v  (cut off)
/// ```
///
/// and with `t` the loop gain around it, the amplifier settles where
/// `x = e + t f(e)`: a quadratic, solved in closed form below. For a small
/// signal the output is `x + x^2 / (2 v (1 + t)^2)` of its normalised gain,
/// which is second harmonic only, growing in proportion to the signal; the
/// feedback divides the JFET's own curvature by the loop gain twice over.
/// On the cut-off side the output bends over smoothly -- its slope reaches
/// zero as the gate reaches pinch-off -- and then holds.
///
/// The two figures that describe it from outside, the curvature `c` and the
/// cut-off level `l`, fix `t` and `v` between them: `1 + t = 1 / (4 c l)` and
/// `v = 8 c l^2`.
///
/// Which way round it bends matters, because it lands on top of the output
/// stage's own curvature. From the preamplifier's JFET to the base of the
/// output transistor the chain does not invert: a common source stage and a
/// common emitter stage in each amplifier invert twice, and the output
/// control between them is a divider. And every device along it -- the
/// JFETs on the square law, the output transistor on its exponential one --
/// stretches the half of the swing that raises its own current. So the
/// JFETs' curvature adds to the output stage's, and it is oriented here as
/// [`Amplifier`]'s Class A shaper is.
#[derive(Clone, Copy)]
struct SquareLaw {
    /// `1 + t`.
    loop_factor: f64,
    /// `2 t / v`.
    slope: f64,
    /// Scales the closed loop gain `t / (1 + t)` back to one.
    normalise: f64,
    /// Where the gate reaches pinch-off, in input terms.
    knee: f64,
    /// The output from there on.
    floor: f64,
}

impl SquareLaw {
    fn new(curve: f64, cutoff: f64) -> Self {
        let loop_factor = 1.0 / (4.0 * curve * cutoff);
        assert!(loop_factor > 1.0, "the curvature needs some feedback");
        let t = loop_factor - 1.0;
        let v = 8.0 * curve * cutoff * cutoff;
        Self {
            loop_factor,
            slope: 2.0 * t / v,
            normalise: loop_factor / t,
            knee: -v * (1.0 + t / 2.0),
            floor: -cutoff,
        }
    }

    /// The Rev A's amplifiers. See [`JFET_CURVE`] and [`JFET_CUTOFF`].
    fn jfet() -> Self {
        Self::new(JFET_CURVE, JFET_CUTOFF)
    }

    #[inline]
    fn process(&self, x: f64) -> f64 {
        // Worked in the JFET's own sense, which stretches positive swings
        // and cuts off on negative ones, and turned over at both ends to sit
        // the way the output stage does.
        let u = -x;
        if u <= self.knee {
            return -self.floor;
        }
        // The smaller root of the quadratic in `e`, in the form that stays
        // accurate as `u` goes to zero.
        let root = (self.loop_factor * self.loop_factor + self.slope * u).sqrt();
        let e = 2.0 * u / (self.loop_factor + root);
        -(u - e) * self.normalise
    }
}

/// The signal preamplifier, between the gain element and the output control.
///
/// It is what the sidechain's divider reads, so its colour, unlike the line
/// amplifier's, is inside the loop's view and in front of the output control.
/// While the unit is limiting the loop holds its output level steady, so the
/// colour it adds stays put however hard the input is driven.
pub struct Preamp {
    law: Option<SquareLaw>,
}

impl Preamp {
    pub fn new(transistors: Transistors) -> Self {
        Self {
            law: match transistors {
                Transistors::Bipolar => None,
                Transistors::Jfet => Some(SquareLaw::jfet()),
            },
        }
    }

    #[inline]
    pub fn process(&self, x: f64) -> f64 {
        match &self.law {
            Some(law) => law.process(x),
            None => x,
        }
    }
}

/// Which output stage a revision fits.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OutputStage {
    /// The 1108 style Class A stage of the early units.
    ClassA,
    /// The push-pull Class AB stage of the later ones.
    ClassAb,
}

/// The line amplifier: its input stage, the output stage and the output
/// transformer.
pub struct Amplifier {
    stage: OutputStage,
    /// How hard the stage is being driven, which sets how much it colours.
    drive: f64,
    /// The Rev A's JFET input. The bipolar inputs add nothing of their own.
    input: Option<SquareLaw>,
    /// Transformer band limits.
    coupling: OnePole,
    bandwidth: OnePole,
}

impl Amplifier {
    pub fn new(stage: OutputStage, drive: f64, transistors: Transistors, sample_rate: f64) -> Self {
        let mut amp = Self {
            stage,
            drive,
            input: match transistors {
                Transistors::Bipolar => None,
                Transistors::Jfet => Some(SquareLaw::jfet()),
            },
            coupling: OnePole::default(),
            bandwidth: OnePole::default(),
        };
        amp.set_sample_rate(sample_rate);
        amp
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        // The output transformer is what sets both ends of the response.
        //
        // Both corners sit far enough out that the unit holds its published
        // 20 Hz to 20 kHz within a decibel with room to spare. A single pole
        // falls away slowly, so the corner has to be well clear of the band
        // edge to stay inside the tolerance there: 38 kHz put 20 kHz at
        // -1.05 dB, which is outside it.
        self.coupling.set_cutoff(5.0, sample_rate);
        self.bandwidth.set_cutoff(55e3, sample_rate);
    }

    #[inline]
    pub fn process(&mut self, sample: f64) -> f64 {
        let mut x = self.coupling.highpass(sample);
        if let Some(input) = &self.input {
            x = input.process(x);
        }
        let shaped = match self.stage {
            OutputStage::ClassA => self.class_a(x),
            OutputStage::ClassAb => self.class_ab(x),
        };
        self.bandwidth.lowpass(shaped)
    }

    /// Class A: no crossover region, and asymmetric, so it makes second
    /// harmonic before it makes third.
    ///
    /// The stage is a long way from its rails at any level the unit is meant
    /// to run at, so it stays close to linear until it is driven hard. It had
    /// been carrying the whole unit's colour, which put a Rev D at 1.8 % with
    /// the gain element idle -- three times its own specification -- and left
    /// nothing for the gain reduction to add. The colour belongs in the FET.
    #[inline]
    fn class_a(&self, x: f64) -> f64 {
        let k = 0.30 + self.drive * 0.85;
        // The offset is what makes it asymmetric; it is removed afterwards so
        // the stage does not pass DC.
        let bias = 0.110;
        let rest = (k * bias).tanh();
        (((x + bias) * k).tanh() - rest) / (k * (1.0 - rest * rest))
    }

    /// Class AB: mostly third harmonic, with a small crossover region where
    /// the halves hand over.
    ///
    /// Push-pull cancels the even harmonics, but only as well as the two
    /// halves are matched, and a real pair never is. Cancelling them outright
    /// left the Rev F with no second harmonic at all and made it read 35 times
    /// cleaner than a Rev D, which is not a revision difference but an
    /// idealisation, so a little of the imbalance is kept.
    #[inline]
    fn class_ab(&self, x: f64) -> f64 {
        let k = 0.24 + self.drive * 0.70;
        const IMBALANCE: f64 = 0.028;
        let rest = (k * IMBALANCE).tanh();
        let shaped = (((x + IMBALANCE) * k).tanh() - rest) / (k * (1.0 - rest * rest));
        // Where the two halves hand over, both conduct and the gain shifts a
        // little. With the stage's feedback around it that is all a biased
        // push-pull pair does there: the gain changes by a fraction, it does
        // not go away. This used to be a dead band, which scaled anything
        // under -56 dBFS down in proportion to its own size -- a tone at
        // -60 dBFS came out 5.5 dB quieter with 20 % distortion, and the
        // unit's own noise all but vanished. Quiet material went through the
        // cleanest revision gated.
        const CROSSOVER_WIDTH: f64 = 0.002;
        const CROSSOVER_DEPTH: f64 = 0.01;
        let handover = (-(shaped / CROSSOVER_WIDTH).powi(2)).exp();
        shaped * (1.0 - CROSSOVER_DEPTH * handover)
    }

    pub fn reset(&mut self) {
        self.coupling.reset();
        self.bandwidth.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// For a small signal the stage is `x - c x^2`: second order only, at
    /// the curvature it was built with.
    #[test]
    fn a_small_signal_sees_the_curvature() {
        let law = SquareLaw::new(0.012, 2.0);
        for x in [1e-3, 1e-2, -1e-2] {
            let bend = law.process(x) - x;
            let wanted = -0.012 * x * x;
            assert!(
                (bend - wanted).abs() < 0.05 * wanted.abs(),
                "at {x}: {bend:e} against {wanted:e}"
            );
        }
        assert_eq!(law.process(0.0), 0.0);
    }

    /// The output rises all the way to the cut-off and then holds, arriving
    /// there flat rather than on a corner.
    #[test]
    fn it_bends_over_smoothly_into_the_cut_off() {
        let law = SquareLaw::new(0.012, 2.0);
        let mut last = f64::NEG_INFINITY;
        for i in -4000..=4000 {
            let y = law.process(i as f64 * 1e-3);
            assert!(y >= last, "not monotonic at {}", i as f64 * 1e-3);
            last = y;
        }
        assert_eq!(law.process(10.0), 2.0);
        let knee = -law.knee;
        let h = 1e-6;
        let slope = (law.process(knee) - law.process(knee - h)) / h;
        assert!(slope.abs() < 0.01, "slope {slope} at the cut-off");
        assert!((law.process(knee - h) - 2.0).abs() < 1e-5);
    }

    #[test]
    fn a_bipolar_preamplifier_passes_the_signal_unchanged() {
        let preamp = Preamp::new(Transistors::Bipolar);
        for x in [-3.0, -0.1, 0.0, 0.5, 4.0] {
            assert_eq!(preamp.process(x), x);
        }
    }
}
