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
/// -25 and -26 dB for 20:1, 12:1 and 8:1. For 4:1 the table gives no figure,
/// because that knee is too soft to have one, but the text does: "in the 4:1
/// compression ratio, the lowest threshold is -30 dB" -- where the reduction
/// begins. The 4:1's knee is the widest (see [`diode_knee_db`]), and with its
/// centre here its lower edge lands on -30 dB. That also puts the centre
/// within a decibel of where the table's output column, falling a decibel a
/// step to +7 dBm, would put it.
pub const THRESHOLD_OFFSETS_DB: [f64; 4] = [-3.6, -2.0, -1.0, 0.0];

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

/// The signal half of the ratio switch bank, from ground up, in kilohms: R22,
/// R21, R20, R19 and R78 of the Rev D schematic. The preamplifier's output
/// is at the top; each button connects the junction above its own resistor
/// -- 4:1 above R22, 8:1 above R21, 12:1 above R20, 20:1 above R19 -- to a
/// common line into the gain reduction control amplifier.
///
/// So pressing several buttons does not put anything in parallel. It joins
/// their junctions, which shorts out every resistor between the lowest and
/// the highest one pressed: the buttons in between change nothing, which is
/// Universal Audio's own observation that "only the 'outside' ratios are
/// relevant", and all four in shorts three of the ladder's five resistors.
const SIGNAL_LADDER_KOHM: [f64; 5] = [47.0, 56.0, 56.0, 68.0, 56.0];

/// The bias half of the bank, in ohms: R61, R62 and R63, between the 4:1 and
/// 8:1, 8:1 and 12:1, and 12:1 and 20:1 contacts. It hangs off the network
/// that sets the gate's resting bias, down to the -10 V rail, and pressing
/// several buttons shorts the part of it between the outermost two.
const BIAS_LADDER_OHM: [f64; 3] = [470.0, 560.0, 1500.0];

/// What shortening the bias ladder does to the gate's resting bias, from a
/// circuit simulation of all-button mode posted to GroupDIY by ioplex: the
/// ladder draws 1.62 mA with one button in and 2.98 mA with all four, and
/// the control voltage's resting point moves from -2.0 V to -3.2 V. The rail
/// is the manual's -10 V. [`rest_bias_v`] fits a source and resistance
/// behind the ladder to both points, so partial combinations fall between.
const REST_ONE_BUTTON_V: f64 = -2.0;
const REST_ALL_BUTTONS_V: f64 = -3.2;
const BIAS_RAIL_V: f64 = -10.0;
const LADDER_ONE_BUTTON_MA: f64 = 1.62;
const LADDER_ALL_BUTTONS_MA: f64 = 2.98;

/// Decibels of control a volt of gate bias is worth.
///
/// The gate rests at the edge of conduction -- the manual's Q-bias
/// adjustment sets it there, "slightly into conduction" -- and fully on
/// about 2 V above, a span that holds the unit's roughly 50 dB of gain
/// reduction. Nothing documents how evenly, so it is taken as even: 25 dB a
/// volt, and the 1.2 V all four buttons drag the gate down is 30 dB of
/// control the sidechain has to supply before the FET starts to open.
const CONTROL_DB_PER_VOLT: f64 = 25.0;

/// How much the gate's shifted bias raises the loop gain, as a fraction, at
/// the full shift all four buttons cause.
///
/// The rectifier works in volts, not decibels, so the loop gain it gives in
/// decibels grows with the amplitude it is working at -- and with the gate
/// resting lower, it has to be driven 1.2 V further for the same reduction.
/// How much that raises the gain depends on circuit values the manual does
/// not give, so this one figure is fitted: read off the ladder alone, all
/// four measure 10.5:1, and the manual puts the mode "somewhere between 12:1
/// and 20:1". Half as much gain again puts it at 15:1, the middle of that.
const DEAD_ZONE_LOOP_GAIN: f64 = 0.5;

/// How much further the gain element bends the signal with the gate pulled
/// off its null, at the full shift all four buttons cause.
///
/// The trimmer cancels the FET's distortion by feeding the gate half the
/// drain voltage. ioplex's simulation shows the extra load on the bias point
/// distorting that correction itself, and Universal Audio's description of
/// the mode is that "distortion increases radically". How much is a voicing
/// choice; `all_buttons_is_dirtier_than_a_plain_ratio` holds the direction.
const ALL_BUTTON_FET_SHIFT: f64 = 2.0;

/// The lowest and highest ratio buttons pressed, as indices into [`RATIOS`].
fn span(buttons: [bool; 4]) -> Option<(usize, usize)> {
    let low = buttons.iter().position(|pressed| *pressed)?;
    let high = buttons.iter().rposition(|pressed| *pressed)?;
    Some((low, high))
}

/// The share of the preamplifier's output the signal ladder passes to the
/// sidechain with the buttons from `low` to `high` pressed.
pub fn ladder_tap(low: usize, high: usize) -> f64 {
    let below: f64 = SIGNAL_LADDER_KOHM[..=low].iter().sum();
    let shorted: f64 = SIGNAL_LADDER_KOHM[low + 1..=high].iter().sum();
    let total: f64 = SIGNAL_LADDER_KOHM.iter().sum();
    below / (total - shorted)
}

/// A single button's figure, read at a share of signal `tap` between the
/// buttons' own shares on the ladder.
fn along_ladder(tap: f64, figure: impl Fn(usize) -> f64) -> f64 {
    let taps: [f64; 4] = std::array::from_fn(|i| ladder_tap(i, i));
    let upper = (1..taps.len())
        .find(|&i| tap <= taps[i])
        .unwrap_or(taps.len() - 1);
    let lower = upper - 1;
    let along = ((tap - taps[lower]) / (taps[upper] - taps[lower])).clamp(0.0, 1.0);
    figure(lower) + (figure(upper) - figure(lower)) * along
}

/// The gate's resting bias with the buttons from `low` to `high` pressed, in
/// volts.
pub fn rest_bias_v(low: usize, high: usize) -> f64 {
    let one = (REST_ONE_BUTTON_V - BIAS_RAIL_V) / (LADDER_ONE_BUTTON_MA * 1e-3);
    let all = (REST_ALL_BUTTONS_V - BIAS_RAIL_V) / (LADDER_ALL_BUTTONS_MA * 1e-3);
    // The simulation's ladder loses a little more than the schematic's three
    // resistors add up to; scale so all four lands on its figure exactly.
    let schematic: f64 = BIAS_LADDER_OHM.iter().sum();
    let shorted: f64 = BIAS_LADDER_OHM[low..high].iter().sum::<f64>() * (one - all) / schematic;
    // A source `v` behind `r` into the ladder, fitted to both points.
    let (a, b) = (
        REST_ONE_BUTTON_V - BIAS_RAIL_V,
        REST_ALL_BUTTONS_V - BIAS_RAIL_V,
    );
    let r = (one * all * (a - b)) / (b * one - a * all);
    let v = a * (one + r) / one;
    BIAS_RAIL_V + v * (one - shorted) / (one - shorted + r)
}

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
    /// A combination shorts the signal ladder between its outermost buttons
    /// and passes the sidechain a share of the signal between theirs, so it
    /// behaves as a switch position between them: its ratio is read off the
    /// single buttons' at that share. All four pass 0.456 of the preamplifier
    /// output, between the 8:1's 0.364 and the 12:1's 0.562, which puts the
    /// loop near 10:1; the gate's shifted bias then raises it (see
    /// [`DEAD_ZONE_LOOP_GAIN`]) into the manual's "somewhere between 12:1 and
    /// 20:1". Reading a combination as its highest button's ratio instead
    /// measured all four at 23:1 to 28:1, outside that.
    pub fn sidechain_gain(&self) -> Option<f64> {
        let full_drop = rest_bias_v(3, 3) - rest_bias_v(0, 3);
        span(self.buttons).map(|(low, high)| {
            along_ladder(ladder_tap(low, high), |i| RATIOS[i] - 1.0)
                * (1.0 + DEAD_ZONE_LOOP_GAIN * self.bias_drop_v() / full_drop)
        })
    }

    /// The ratio the loop settles at before the knee is taken into account.
    pub fn ratio(&self) -> Option<f64> {
        self.sidechain_gain().map(|k| k + 1.0)
    }

    /// Where the pressed buttons put the threshold, relative to the 20:1's,
    /// in dB, read off the single buttons' at the share of signal the ladder
    /// passes, as [`Self::sidechain_gain`] is. The gate's shifted bias raises
    /// it further, through [`Self::dead_zone_db`].
    pub fn threshold_offset_db(&self) -> f64 {
        match span(self.buttons) {
            Some((low, high)) => along_ladder(ladder_tap(low, high), |i| THRESHOLD_OFFSETS_DB[i]),
            None => 0.0,
        }
    }

    /// How far below its calibrated rest the gate sits, in volts. Zero with a
    /// single button in; 1.2 V with all four.
    pub fn bias_drop_v(&self) -> f64 {
        match span(self.buttons) {
            Some((low, high)) => rest_bias_v(high, high) - rest_bias_v(low, high),
            None => 0.0,
        }
    }

    /// The control the sidechain has to supply before the gain element starts
    /// to open, in dB. See [`CONTROL_DB_PER_VOLT`].
    pub fn dead_zone_db(&self) -> f64 {
        self.bias_drop_v() * CONTROL_DB_PER_VOLT
    }
}

/// Width of the knee the rectifier diodes give a sidechain gain `k` whose
/// threshold sits `offset_db` from the 20:1's, in dB.
///
/// The diodes' turn-on spans the same voltage whatever the button, so the
/// knee in decibels widens as the signal at the diodes at threshold shrinks.
/// That signal is the threshold level times the share of it the ratio
/// divider passes, which is proportional to `k`. The 4:1 passes the least
/// and has the lowest threshold, so its signal at the diodes is about a tenth
/// of the 20:1's and its knee about ten times as wide -- the soft knee the
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
    /// The highest the gain reduction meter has read since it was last
    /// taken, in dB. Negative when the gate is resting below its calibrated
    /// point, which is where all-button mode leaves it.
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
            meter_db: f64::NEG_INFINITY,
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

        let marked = self.controls.sidechain_gain().unwrap_or(0.0);
        let ratio = (marked * self.revision.ratio_accuracy).max(0.0);
        let full_drop = rest_bias_v(3, 3) - rest_bias_v(0, 3);
        let drop = self.controls.bias_drop_v();

        // The gate pulled off its null is what makes a combination dirty.
        self.fet
            .set_bias_shift(1.0 + (ALL_BUTTON_FET_SHIFT - 1.0) * drop / full_drop);

        let offset = self.controls.threshold_offset_db();

        self.detector.set_timing(Timing {
            k: ratio,
            attack: detector::knob_to_time(
                self.controls.attack,
                detector::ATTACK_FASTEST,
                detector::ATTACK_SLOWEST,
            ),
            release: detector::knob_to_time(
                self.controls.release,
                detector::RELEASE_FASTEST,
                detector::RELEASE_SLOWEST,
            ),
            threshold: detector::THRESHOLD_DB + offset,
            knee: diode_knee_db(marked, offset),
            dead_zone: self.controls.dead_zone_db(),
        });
    }

    /// Gain reduction being applied right now, in dB.
    pub fn gain_reduction_db(&self) -> f64 {
        self.reduction_db
    }

    /// What the gain reduction meter reads, in dB, and resets the peak hold.
    /// With the gain element out of circuit there is nothing to read, which
    /// is no reduction.
    pub fn take_meter(&mut self) -> f32 {
        let value = std::mem::replace(&mut self.meter_db, f64::NEG_INFINITY);
        if value.is_finite() {
            value as f32
        } else {
            0.0
        }
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
            *meter_db = meter_db.max(detector.control_db());
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
        self.meter_db = f64::NEG_INFINITY;
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

#[cfg(test)]
mod switch_bank {
    use super::*;

    /// The single buttons' taps from the Rev D values, and all four's.
    #[test]
    fn the_signal_ladder_divides_as_drawn() {
        let taps: Vec<f64> = (0..4).map(|i| ladder_tap(i, i)).collect();
        for (tap, wanted) in taps.iter().zip([0.166, 0.364, 0.562, 0.802]) {
            assert!((tap - wanted).abs() < 0.001, "{tap:.3} against {wanted}");
        }
        assert!((ladder_tap(0, 3) - 0.456).abs() < 0.001);
    }

    /// The fit lands on both of the simulation's points.
    #[test]
    fn the_gate_rests_where_the_simulation_puts_it() {
        for i in 0..4 {
            assert!((rest_bias_v(i, i) - REST_ONE_BUTTON_V).abs() < 1e-9);
        }
        assert!((rest_bias_v(0, 3) - REST_ALL_BUTTONS_V).abs() < 1e-9);
        // A partial combination shorts less of the ladder and moves it less.
        let partial = rest_bias_v(1, 3);
        assert!(partial < REST_ONE_BUTTON_V && partial > REST_ALL_BUTTONS_V);
    }

    /// A single button is exactly its marking, and the manual's thresholds.
    #[test]
    fn a_single_button_is_its_marking() {
        for (i, ratio) in RATIOS.iter().enumerate() {
            let mut buttons = [false; 4];
            buttons[i] = true;
            let controls = Controls {
                buttons,
                ..Controls::default()
            };
            assert_eq!(controls.ratio(), Some(*ratio));
            assert!((controls.threshold_offset_db() - THRESHOLD_OFFSETS_DB[i]).abs() < 1e-12);
            assert_eq!(controls.dead_zone_db(), 0.0);
        }
    }
}
