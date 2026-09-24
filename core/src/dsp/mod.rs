//! The Comp76Fx limiting amplifier.
//!
//! One signal path, shared by every revision, with the differences between
//! them expressed as a [`Revision`] rather than as separate code.

pub mod amp;
pub mod delay;
pub mod detector;
pub mod fet;
pub mod oversample;
pub mod revisions;

use amp::OnePole;
pub use amp::{Amplifier, OutputStage};
pub use delay::Delay;
pub use detector::{Detector, Timing};
pub use fet::Fet;
pub use oversample::Oversampler;
pub use revisions::{REV_A, REV_D, REV_F};

/// Latency every channel has, in samples at the host rate, whatever the
/// oversampling is set to: the longest the oversampler can take, with the
/// difference made up by a plain delay. The host is told this once, so
/// changing the quality never shifts the track against the rest of the
/// session, and the dry signal can be held back by the same amount.
pub const LATENCY: u32 = oversample::MAX_LATENCY;

/// Ratios the four front panel buttons select.
pub const RATIOS: [f64; 4] = [4.0, 8.0, 12.0, 20.0];

/// Where each button puts the threshold, in dB relative to the 20:1's
/// [`detector::THRESHOLD_DB`], in the order of [`RATIOS`].
///
/// The buttons switch a DC divider that biases the rectifier diodes as well
/// as the signal divider that feeds them, so each ratio has a threshold of its
/// own, and the manual says so: "selecting higher ratios also raises the
/// threshold level". Its table puts the input at minimum threshold at -24,
/// -25 and -26 dB for 20:1, 12:1 and 8:1. For 4:1 it gives no figure, because
/// that knee is too soft to have one, but the output at threshold falls a
/// decibel a step down the table (+10, +9, +8, +7 dBm), which puts the 4:1 a
/// decibel below the 8:1.
pub const THRESHOLD_OFFSETS_DB: [f64; 4] = [-3.0, -2.0, -1.0, 0.0];

/// Width of the knee the rectifier diodes give the 20:1 button, in dB at the
/// rectifier. The other buttons are wider; see [`diode_knee_db`].
///
/// A biased diode does not switch on at a point, it turns on over a fixed
/// span of voltage. How many decibels of signal that span covers depends on
/// how large the signal at the diodes is at threshold: the bigger it is, the
/// sharper the corner. That fixes how the buttons' knees compare; the manual
/// gives no width, so the size of them all is bounded by its ratio test. It
/// measures each ratio from 1 dB of limiting, except the 4:1, which it
/// measures from 3 dB "because of the soft knee in the threshold circuit for
/// this ratio", and holds each to 20 %. Much wider than this and the 8:1
/// fails from 1 dB as well; much narrower and the 4:1 would have had no need
/// of the exception. At half a decibel the 4:1 reads worst of the four from
/// 1 dB, 12.6 % out on a Rev D, and 3.4 % out from 3; `tests/threshold.rs`
/// runs the manual's procedure.
const DIODE_KNEE_DB: f64 = 0.5;

/// Corner of the coupling capacitor the sidechain amplifier is fed through,
/// in Hz.
///
/// The manual has the control amplifier start with a phase inverter fed from
/// a divider on the preamplifier's output. A transistor stage biased from its
/// own supply takes its signal through a capacitor, so the rectifier never
/// sees DC. That matters here because the gain element's even order
/// distortion leaves a DC offset on the signal, which lifts one half of the
/// waveform above the other, and the full-wave rectifier follows whichever
/// peaks higher. With the offset let through, the ratios read up to 6 % high
/// at 20:1, more the harder the unit worked; blocked, they land within about
/// a percent of their markings. The value is not given, so it is set low
/// enough to leave the audio band alone, like the output coupling.
pub const SIDECHAIN_COUPLING_HZ: f64 = 5.0;

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

/// What separates one revision from another. The three that ship are in
/// [`revisions`].
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

impl Revision {
    /// The same circuit with its noise switched off, so a measurement reads
    /// the circuit rather than the noise floor.
    pub const fn without_noise(self) -> Self {
        Self {
            noise_floor_db: -400.0,
            ..self
        }
    }
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

    /// Where the pressed buttons put the threshold, relative to the 20:1's,
    /// in dB. With several in, their bias taps are in parallel as their
    /// signal taps are, so each pulls by its conductance -- which is its
    /// share of the sidechain gain -- and the stiffest has the most say.
    pub fn threshold_offset_db(&self) -> f64 {
        let (weighted, total) = RATIOS
            .iter()
            .zip(THRESHOLD_OFFSETS_DB)
            .zip(self.buttons)
            .filter(|(_, pressed)| *pressed)
            .fold((0.0, 0.0), |(weighted, total), ((ratio, offset), _)| {
                let k = ratio - 1.0;
                (weighted + k * offset, total + k)
            });
        if total > 0.0 {
            weighted / total
        } else {
            0.0
        }
    }
}

/// Width of the knee the rectifier diodes give a sidechain gain `k` whose
/// threshold sits `offset_db` from the 20:1's, in dB.
///
/// The diodes' turn-on spans the same voltage whatever the button, so the
/// knee in decibels widens as the signal at the diodes at threshold shrinks.
/// That signal is the threshold level times the share of it the ratio
/// divider passes, which is proportional to `k`. The 4:1 passes the least
/// and has the lowest threshold, so its signal at the diodes is about a ninth
/// of the 20:1's and its knee about nine times as wide -- the soft knee the
/// manual names for that ratio alone.
pub fn diode_knee_db(k: f64, offset_db: f64) -> f64 {
    if k <= 0.0 {
        return 0.0;
    }
    let reference = RATIOS[3] - 1.0;
    let at_diodes = k * db_to_gain(offset_db);
    DIODE_KNEE_DB * reference / at_diodes
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
    /// Makes the oversampler's latency up to [`LATENCY`] at every setting.
    pad: Delay,
    /// The capacitor the sidechain amplifier is fed through.
    coupling: OnePole,
    /// Gains the controls and the oversampling set, worked out when they
    /// change rather than on every sample.
    input_gain: f64,
    output_gain: f64,
    noise_gain: f64,
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
            pad: Delay::new(0, LATENCY as usize),
            coupling: OnePole::default(),
            input_gain: 1.0,
            output_gain: 1.0,
            noise_gain: 0.0,
        };
        channel.retune();
        channel.apply_controls();
        channel
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.retune();
        self.reset();
    }

    pub fn set_oversampling(&mut self, factor: usize) {
        if factor == self.oversampler.factor() {
            return;
        }
        self.oversampler.set_factor(factor);
        self.retune();
        self.reset();
    }

    /// Everything that follows from the rate the circuit runs at.
    fn retune(&mut self) {
        let factor = self.oversampler.factor();
        let internal = self.sample_rate * factor as f64;
        self.detector.set_sample_rate(internal);
        self.amp.set_sample_rate(internal);
        self.coupling.set_cutoff(SIDECHAIN_COUPLING_HZ, internal);
        self.pad
            .set_delay((LATENCY - self.oversampler.latency()) as usize);
        // The noise is white at the internal rate, and the way back down to
        // the host rate keeps only the audio band, which is `1 / factor` of
        // it. Scaled by the root of the factor, the noise that is left is the
        // same at every setting, so the quality switch cannot move the floor.
        self.noise_gain = db_to_gain(self.revision.noise_floor_db) * (factor as f64).sqrt();
    }

    /// Always [`LATENCY`], whatever the oversampling.
    pub fn latency(&self) -> u32 {
        LATENCY
    }

    pub fn set_controls(&mut self, controls: Controls) {
        if controls == self.controls {
            return;
        }
        self.controls = controls;
        self.apply_controls();
    }

    fn apply_controls(&mut self) {
        self.input_gain = db_to_gain(self.controls.input_db);
        self.output_gain = db_to_gain(self.controls.output_db);
        // With no button in, the gain element is out of circuit and the
        // sidechain with it. Letting go of what it held means that when a
        // button goes back in, the reduction it starts from is the reduction
        // the gain element is actually applying, which is none.
        if self.controls.ratio().is_none() {
            self.detector.reset();
            self.reduction_db = 0.0;
        }

        let all = self.controls.all_buttons();
        let blend = self.controls.combination();
        let marked = self.controls.sidechain_gain().unwrap_or(0.0);
        let ratio = (marked * self.revision.ratio_accuracy).max(0.0);
        let offset = self.controls.threshold_offset_db();

        // The bias shift is what makes a combination dirty as well as slow,
        // and it grows with how many buttons are in.
        self.fet
            .set_bias_shift(1.0 + (COMBINED_FET_SHIFT - 1.0) * blend);

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
            threshold: detector::THRESHOLD_DB + offset - COMBINED_THRESHOLD_DB * blend,
            knee: diode_knee_db(marked, offset) + COMBINED_KNEE_DB * blend,
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
        let compressing = self.controls.ratio().is_some();
        let noise_gain = self.noise_gain;

        let Self {
            detector,
            fet,
            amp,
            oversampler,
            reduction_db,
            meter_db,
            noise,
            coupling,
            ..
        } = self;

        let out = oversampler.process(sample as f64 * self.input_gain, &mut |x| {
            if !compressing {
                return amp.process(x) + white(noise) * noise_gain;
            }
            // The gain element runs on what the detector asked for, and the
            // detector is fed what came out, which is how the loop is closed.
            // It is fed from the preamplifier, straight after the gain
            // element and ahead of the output control and the line amplifier,
            // which is where the unit's divider takes it: the output stage's
            // colour is outside the loop, as it is on the hardware.
            let reduced = fet.process(x, -*reduction_db);
            *reduction_db = detector.process_in_loop(coupling.highpass(reduced));
            if *reduction_db > *meter_db {
                *meter_db = *reduction_db;
            }
            amp.process(reduced) + white(noise) * noise_gain
        });

        (self.pad.process(out) * self.output_gain) as f32
    }

    pub fn reset(&mut self) {
        self.detector.reset();
        self.amp.reset();
        self.oversampler.reset();
        self.pad.reset();
        self.coupling.reset();
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
