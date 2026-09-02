//! The Comp76Fx limiting amplifier.
//!
//! One signal path, shared by every revision, with the differences between
//! them expressed as a [`Revision`] rather than as separate code.

pub mod amp;
pub mod detector;
pub mod fet;
pub mod oversample;

pub use amp::{Amplifier, OutputStage};
pub use detector::{Detector, Timing};
pub use fet::Fet;
pub use oversample::Oversampler;

/// Ratios the four front panel buttons select.
pub const RATIOS: [f64; 4] = [4.0, 8.0, 12.0, 20.0];

/// What pressing more than one ratio button does.
///
/// Each button switches its own resistor into the sidechain, so pressing
/// several puts them in parallel and their conductances add: the loop gain
/// goes up, not to whichever button is highest. Treating the highest as the
/// winner made every combination identical to one of its members, which is
/// why two and three buttons did nothing at all and why all four needed
/// special casing to be interesting.
///
/// Three things then follow from how many are in, and all four falls out as
/// the far end of them rather than as a mode of its own:
///
/// * the bias shifts, dragging the operating point down, so the unit is
///   already working where one button would still be waiting;
/// * the knee opens out, so the gain arrives over a range of level rather
///   than at a point -- and because the detector is fed the compressed
///   output, that width is what pulls the measured slope back down into the
///   "somewhere between 12:1 and 20:1" the manual claims for all four; and
/// * the gate is dragged away from where the trimmer nulled it, so the gain
///   element bends the signal further.
///
/// Each is scaled by how many buttons are in beyond the first.
const COMBINED_THRESHOLD_DB: f64 = 11.0;
const COMBINED_KNEE_DB: f64 = 9.0;
const COMBINED_FET_SHIFT: f64 = 6.0;
/// All-button mode slows the attack and speeds the recovery.
const ALL_BUTTON_ATTACK: f64 = 6.0;
const ALL_BUTTON_RELEASE: f64 = 0.55;

/// What separates one revision from another.
#[derive(Clone, Copy)]
pub struct Revision {
    /// Shown on the panel, for example "Rev A".
    pub name: &'static str,
    /// The faceplate the unit was built with. Cosmetic, but it is the first
    /// thing that tells one revision from another across a studio.
    pub finish: Finish,
    /// Folder name for this plugin's own saved presets. Each revision is a
    /// separate plugin, so each keeps its own.
    pub slug: &'static str,
    pub stage: OutputStage,
    /// How hard the output stage is run, and so how much it colours.
    pub amp_drive: f64,
    /// How readily the FET distorts. The units without the low noise circuit
    /// run their FET harder.
    pub fet_drive: f64,
    /// Asymmetry of the FET's operating point.
    pub fet_bias: f64,
    /// Broadband noise the unit contributes, in dB below full scale. The low
    /// noise revisions are the quieter ones, which is what LN meant.
    pub noise_floor_db: f64,
    /// Ratio the sidechain actually reaches, as a fraction of the marked
    /// value. The early units do not quite hit their marks.
    pub ratio_accuracy: f64,
}

/// The faceplate a revision was built with. The units were not restyled on
/// every revision, so a finish covers a run of them: the Bluestripe badge
/// belongs to the earliest, black to the low noise units that followed, and
/// the brushed aluminium panel to the UREI era from Rev F on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Finish {
    /// Black panel with the painted band around the meter. Rev A and B.
    BlueStripe,
    /// Black panel throughout. Rev C to E.
    BlackFace,
    /// Brushed aluminium with black lettering. Rev F onward.
    SilverFace,
}

/// The knob and button positions, all normalised.
#[derive(Clone, Copy, PartialEq)]
pub struct Controls {
    /// Drive into the fixed operating point, in dB.
    pub input_db: f64,
    /// Make-up after the gain element, in dB.
    pub output_db: f64,
    /// `0.0` slowest, `1.0` fastest, as the panel is marked.
    pub attack: f64,
    pub release: f64,
    /// Which ratio buttons are pressed. All four is all-button mode; none at
    /// all is 1:1, which passes the signal through the amplifier untouched by
    /// the gain element.
    pub buttons: [bool; 4],
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            input_db: 0.0,
            output_db: 0.0,
            attack: 0.5,
            release: 0.5,
            buttons: [true, false, false, false],
        }
    }
}

impl Controls {
    pub fn all_buttons(&self) -> bool {
        self.buttons.iter().all(|pressed| *pressed)
    }

    /// How many ratio buttons are in.
    pub fn pressed(&self) -> usize {
        self.buttons.iter().filter(|p| **p).count()
    }

    /// Sidechain gain, which is the ratio less one. `None` when no button is
    /// in and the gain element is out of circuit.
    ///
    /// The buttons' resistors sit in parallel, so their conductances add.
    pub fn sidechain_gain(&self) -> Option<f64> {
        let total: f64 = RATIOS
            .iter()
            .zip(self.buttons)
            .filter(|(_, pressed)| *pressed)
            .map(|(ratio, _)| ratio - 1.0)
            .sum();
        (total > 0.0).then_some(total)
    }

    /// The ratio the loop settles at before the knee is taken into account.
    pub fn ratio(&self) -> Option<f64> {
        self.sidechain_gain().map(|k| k + 1.0)
    }

    /// How far past a single button this combination sits, from zero to one.
    fn combination(&self) -> f64 {
        (self.pressed().saturating_sub(1)) as f64 / (RATIOS.len() - 1) as f64
    }
}

/// One channel of the unit.
pub struct Channel {
    revision: Revision,
    detector: Detector,
    fet: Fet,
    amp: Amplifier,
    oversampler: Oversampler,
    sample_rate: f64,
    controls: Controls,
    /// Gain reduction of the previous sample, which closes the feedback loop.
    reduction_db: f64,
    /// Peak gain reduction since the meter last read it.
    meter_db: f64,
    /// A very small amount of noise, seeded per channel.
    noise: u32,
}

impl Channel {
    pub fn new(revision: Revision, sample_rate: f64, factor: usize, seed: u32) -> Self {
        let oversampler = Oversampler::new(factor);
        let internal = sample_rate * oversampler.factor() as f64;
        let mut channel = Self {
            revision,
            detector: Detector::new(internal),
            fet: Fet::new(revision.fet_drive, revision.fet_bias),
            amp: Amplifier::new(revision.stage, revision.amp_drive, internal),
            oversampler,
            sample_rate,
            controls: Controls::default(),
            reduction_db: 0.0,
            meter_db: 0.0,
            noise: seed | 1,
        };
        channel.apply_controls();
        channel
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        let internal = sample_rate * self.oversampler.factor() as f64;
        self.detector.set_sample_rate(internal);
        self.amp.set_sample_rate(internal);
        self.reset();
    }

    pub fn set_oversampling(&mut self, factor: usize) {
        if factor == self.oversampler.factor() {
            return;
        }
        self.oversampler.set_factor(factor);
        let internal = self.sample_rate * self.oversampler.factor() as f64;
        self.detector.set_sample_rate(internal);
        self.amp.set_sample_rate(internal);
        self.reset();
    }

    pub fn latency(&self) -> u32 {
        self.oversampler.latency()
    }

    pub fn set_controls(&mut self, controls: Controls) {
        if controls == self.controls {
            return;
        }
        self.controls = controls;
        self.apply_controls();
    }

    fn apply_controls(&mut self) {
        let all = self.controls.all_buttons();
        let blend = self.controls.combination();
        let ratio = self
            .controls
            .sidechain_gain()
            .map(|k| (k * self.revision.ratio_accuracy).max(0.0))
            .unwrap_or(0.0);

        // The bias shift is what makes a combination dirty as well as slow,
        // and it grows with how many buttons are in.
        self.fet.set_bias_shift(1.0 + (COMBINED_FET_SHIFT - 1.0) * blend);

        let (attack_scale, release_scale) = if all {
            (ALL_BUTTON_ATTACK, ALL_BUTTON_RELEASE)
        } else {
            (1.0, 1.0)
        };

        self.detector.set_timing(Timing {
            k: ratio,
            attack: detector::knob_to_time(
                self.controls.attack,
                detector::ATTACK_FASTEST,
                detector::ATTACK_SLOWEST,
            ) * attack_scale,
            release: detector::knob_to_time(
                self.controls.release,
                detector::RELEASE_FASTEST,
                detector::RELEASE_SLOWEST,
            ) * release_scale,
            threshold: detector::THRESHOLD_DB - COMBINED_THRESHOLD_DB * blend,
            knee: COMBINED_KNEE_DB * blend,
        });
    }

    /// Gain reduction being applied right now, in dB.
    pub fn gain_reduction_db(&self) -> f64 {
        self.reduction_db
    }

    /// Gain reduction to show on the meter, in dB, and resets the peak hold.
    pub fn take_meter(&mut self) -> f32 {
        let value = self.meter_db;
        self.meter_db = 0.0;
        value as f32
    }

    #[inline]
    pub fn process(&mut self, sample: f32) -> f32 {
        let input_gain = db_to_gain(self.controls.input_db);
        let output_gain = db_to_gain(self.controls.output_db);
        let compressing = self.controls.ratio().is_some();
        let noise_gain = db_to_gain(self.revision.noise_floor_db);

        let Self {
            detector,
            fet,
            amp,
            oversampler,
            reduction_db,
            meter_db,
            noise,
            ..
        } = self;

        let out = oversampler.process(sample as f64 * input_gain, &mut |x| {
            // The gain element runs on what the detector asked for last
            // sample, which is how the loop is closed.
            let reduced = if compressing {
                fet.process(x, -*reduction_db)
            } else {
                x
            };
            let amplified = amp.process(reduced) + white(noise) * noise_gain;

            if compressing {
                *reduction_db = detector.process(amplified);
                if *reduction_db > *meter_db {
                    *meter_db = *reduction_db;
                }
            } else {
                *reduction_db = 0.0;
            }
            amplified
        });

        (out * output_gain) as f32
    }

    pub fn reset(&mut self) {
        self.detector.reset();
        self.amp.reset();
        self.oversampler.reset();
        self.reduction_db = 0.0;
        self.meter_db = 0.0;
    }
}

#[inline]
pub fn db_to_gain(db: f64) -> f64 {
    10.0_f64.powf(db / 20.0)
}

/// A cheap uniform noise source, enough for a noise floor 80 dB down.
#[inline]
fn white(state: &mut u32) -> f64 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state as f64 / u32::MAX as f64) * 2.0 - 1.0
}
