//! The gain element.
//!
//! A field effect transistor sits as the lower leg of a divider with the audio
//! across it, and the sidechain's control voltage sets its channel resistance.
//! In the triode region a JFET's drain current is
//!
//! ```text
//!     Id = k [ 2 (Vgs - Vt) Vds - Vds^2 ]
//! ```
//!
//! so the channel conductance
//!
//! ```text
//!     g = dId/dVds = 2k [ (Vgs - Vt) - Vds ]
//! ```
//!
//! falls linearly with the drain-source voltage. The resistance therefore
//! moves with the very signal it is passing, and the divider ratio moves with
//! it. That is a squared term in the transfer, so it is second harmonic, and
//! it grows both with the signal across the channel and with how far the
//! channel has been pinched. It is most of why an 1176 sounds like an 1176
//! when it is working hard, and all of why all-button mode sounds the way it
//! does.
//!
//! The hardware has a trimmer that feeds half the drain voltage back to the
//! gate, which cancels the `Vds` term and nulls that harmonic. How well it is
//! set is one of the real differences between revisions and between individual
//! units, and it is what `modulation` stands for here.

/// How far the channel modulation grows as the gate pinches it off, per dB of
/// gain reduction. Pinching lowers `Vgs - Vt`, so the `Vds` term counts for
/// proportionally more and the same signal bends the channel further.
const PINCH_PER_DB: f64 = 0.015;

/// Overall depth. Calibrated so a Rev D holds about 0.23 % at light gain
/// reduction, inside its published half a percent, and climbs to a bit under
/// 2 % when it is being leaned on hard.
const DEPTH: f64 = 0.20;


pub struct Fet {
    /// How far this revision's trimmer leaves the channel from a null.
    modulation: f64,
    /// How sharply that grows as the channel is pinched.
    pinch: f64,
    /// Raised in all-button mode.
    shift: f64,
}

impl Fet {
    pub fn new(drive: f64, bias: f64) -> Self {
        Self {
            modulation: drive * DEPTH,
            // The revisions differ in where the gate sits as well as in how
            // well they are trimmed, so they pinch at slightly different rates.
            pinch: PINCH_PER_DB * (1.0 + bias),
            shift: 1.0,
        }
    }

    /// How far the gate has been dragged from where the trimmer nulled it.
    /// One is a trimmed unit; more than one is a combination of ratio buttons
    /// pulling the bias about.
    pub fn set_bias_shift(&mut self, shift: f64) {
        self.shift = shift.max(1.0);
    }

    /// Applies `gain_db` of attenuation to a sample, bending it the way the
    /// channel resistance does.
    #[inline]
    pub fn process(&self, sample: f64, gain_db: f64) -> f64 {
        let attenuation = crate::dsp::db_to_gain(gain_db);
        let attenuated = sample * attenuation;

        // Nothing is being asked of the FET, so nothing is coloured by it.
        if gain_db > -0.01 {
            return attenuated;
        }

        let reduction = -gain_db;
        let lambda = self.modulation * self.shift * (1.0 + reduction * self.pinch);

        // The signal across the channel is the attenuated one, and `1 - a` is
        // how much of the divider the series leg now holds: with the channel
        // wide open there is no divider to modulate and no distortion at all,
        // which is why an idle 1176 measures clean.
        let across = attenuated;
        let bend = (lambda * across * (1.0 - attenuation)).clamp(-0.85, 0.85);

        attenuated / (1.0 - bend)
    }
}
