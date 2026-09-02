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

/// What all four buttons at once comes to. The manual puts it "somewhere
/// between 12:1 and 20:1", and the bias shift takes the timing with it.
const ALL_BUTTON_RATIO: f64 = 15.0;
/// How wide the knee opens out in all-button mode. The shifted bias leaves the
/// sidechain without a definite point at which it starts working, so the gain
/// comes down over a range of level rather than at a threshold, and the front
/// of a transient is through before much of it has arrived.
///
/// The width sets the ratio as well as the shape, because the detector is fed
/// the compressed output and so only ever sits a little above the operating
/// point, by about the reduction divided by the sidechain gain, which is
/// inside the knee at any setting. Working the loop through, the slope comes
/// out as `1 + k (u + w / 2) / w`, so a wide knee is not merely soft, it is a
/// lower ratio: 14 dB of it measured 5.7:1. Three puts it at about 14.5:1, inside
/// the "between 12:1 and 20:1" the manual claims, and leaves the ratio
/// climbing with how hard the unit is driven, which is the part that makes
/// the mode grab harder the more you feed it.
const ALL_BUTTON_KNEE: f64 = 3.0;
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

    /// The ratio the sidechain is set to, or `None` when no button is in and
    /// the gain element is out of circuit.
    pub fn ratio(&self) -> Option<f64> {
        if self.all_buttons() {
            return Some(ALL_BUTTON_RATIO);
        }
        // With more than one button in, the highest wins, as the buttons are
        // switching one network between taps.
        RATIOS
            .iter()
            .zip(self.buttons)
            .filter(|(_, pressed)| *pressed)
            .map(|(ratio, _)| *ratio)
            .fold(None, |best: Option<f64>, ratio| {
                Some(best.map_or(ratio, |b| b.max(ratio)))
            })
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
        let ratio = self
            .controls
            .ratio()
            .map(|r| ((r - 1.0) * self.revision.ratio_accuracy).max(0.0))
            .unwrap_or(0.0);

        // The bias shift is what makes all-button mode dirty as well as slow.
        self.fet.set_all_buttons(all);

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
            knee: if all { ALL_BUTTON_KNEE } else { 0.0 },
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
