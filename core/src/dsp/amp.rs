//! The amplifier stages and the iron around them.
//!
//! The revisions differ here more than anywhere else. The early units run a
//! Class A output, which has no crossover region at all and saturates
//! gradually and asymmetrically. Later ones use a push-pull Class AB stage,
//! which is cleaner and more symmetrical but has a crossover region of its own.
//! Both sit behind a transformer, which rounds the top and softens the bottom.

/// A one pole filter for the band limits of the transformers.
#[derive(Default, Clone, Copy)]
struct OnePole {
    a: f64,
    z: f64,
}

impl OnePole {
    fn set_cutoff(&mut self, freq: f64, sample_rate: f64) {
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
    fn highpass(&mut self, x: f64) -> f64 {
        x - self.lowpass(x)
    }

    fn reset(&mut self) {
        self.z = 0.0;
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

pub struct Amplifier {
    stage: OutputStage,
    /// How hard the stage is being driven, which sets how much it colours.
    drive: f64,
    /// Transformer band limits.
    coupling: OnePole,
    bandwidth: OnePole,
}

impl Amplifier {
    pub fn new(stage: OutputStage, drive: f64, sample_rate: f64) -> Self {
        let mut amp = Self {
            stage,
            drive,
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
        let x = self.coupling.highpass(sample);
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
        // A slight dead band where the two halves meet.
        const CROSSOVER: f64 = 0.0016;
        let magnitude = shaped.abs();
        if magnitude < CROSSOVER {
            shaped * (magnitude / CROSSOVER)
        } else {
            shaped
        }
    }

    pub fn reset(&mut self) {
        self.coupling.reset();
        self.bandwidth.reset();
    }
}
