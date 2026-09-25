//! The three revisions the plugins model.
//!
//! Defined here rather than in the crates that export them, so that the tests
//! and the bench measure exactly the circuits that ship instead of copies
//! that can drift from them.
//!
//! What is grounded and what is not. The switch banks are read off each
//! revision's schematic in the UREI manual. The noise is the manual's own
//! signal to noise figure for the low noise units, with the Rev A 3.4 dB
//! noisier, which is what a side by side measurement of a Rev A and a Rev D
//! found (DIY Recording Equipment's revision guide). The output stages are
//! the revision history's: Class A until the Rev F replaced it with a
//! push-pull amplifier. How hard each FET and output stage is driven is not
//! documented beyond its direction -- the Rev A "imparts more harmonic
//! distortion", having neither the low noise circuit that lowers the voltage
//! across the FET nor the Q-bias trimmer that nulls it, and the Rev F
//! "measures the lowest harmonic distortion of any revision" -- so those
//! figures are voicing, held in that order by the tests.

use super::{Finish, OutputStage, Revision, LN_BANK, REV_A_BANK};

/// The low noise units' signal to noise at the 20:1's threshold: the manual
/// takes its noise specification, under -57 dBu from 30 Hz to 15.7 kHz with
/// the controls fully up, and works it out as "a s/n ratio of 80 db".
const LN_SIGNAL_TO_NOISE_DB: f64 = 80.0;

/// The Bluestripe: the original, before the low noise circuit existed.
///
/// Its FET sees more of the signal and has no trimmer to null it, and its
/// preamplifier and line amplifier are built with FETs, the only revision to
/// be, so it is the dirtiest of the three and the noisiest, by 3.4 dB. Its
/// signal ladder passes the sidechain a little more signal than the later
/// bank does, so its ratios sit about 3 % steeper than their markings.
pub const REV_A: Revision = Revision {
    name: "Rev A",
    slug: "comp76fx-rev-a",
    finish: Finish::BlueStripe,
    stage: OutputStage::ClassA,
    amp_drive: 0.62,
    fet_drive: 1.55,
    fet_bias: 0.30,
    signal_to_noise_db: LN_SIGNAL_TO_NOISE_DB - 3.4,
    bank: REV_A_BANK,
};

/// The Blackface, and the one most people mean by the name: transformer
/// input, the low noise circuitry and a Class A output stage.
pub const REV_D: Revision = Revision {
    name: "Rev D",
    slug: "comp76fx-rev-d",
    finish: Finish::BlackFace,
    stage: OutputStage::ClassA,
    amp_drive: 0.45,
    fet_drive: 1.00,
    fet_bias: 0.12,
    signal_to_noise_db: LN_SIGNAL_TO_NOISE_DB,
    bank: LN_BANK,
};

/// Still a Blackface -- the silver panel came with the Rev H -- with the same
/// low noise front end and switch bank as the D, but a push-pull output stage
/// after the 1109 preamplifier and a different output transformer in place
/// of the Class A one.
pub const REV_F: Revision = Revision {
    name: "Rev F",
    slug: "comp76fx-rev-f",
    finish: Finish::BlackFace,
    stage: OutputStage::ClassAb,
    amp_drive: 0.34,
    fet_drive: 0.62,
    fet_bias: 0.04,
    signal_to_noise_db: LN_SIGNAL_TO_NOISE_DB,
    bank: LN_BANK,
};
