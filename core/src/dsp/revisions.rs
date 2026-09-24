//! The three revisions the plugins model.
//!
//! Defined here rather than in the crates that export them, so that the tests
//! and the bench measure exactly the circuits that ship instead of copies
//! that can drift from them.

use super::{Finish, OutputStage, Revision};

/// The Bluestripe: the original, before the low noise circuit existed.
///
/// Its FET is run harder and its Class A output stage is driven closer to its
/// limits, so it is the loudest, dirtiest and least accurate of the three: the
/// ratios do not quite reach their marked values, and the noise floor is
/// audibly higher. That is the sound people go looking for.
pub const REV_A: Revision = Revision {
    name: "Rev A",
    slug: "comp76fx-rev-a",
    finish: Finish::BlueStripe,
    stage: OutputStage::ClassA,
    amp_drive: 0.62,
    fet_drive: 1.55,
    fet_bias: 0.30,
    // No low noise circuit yet, and it shows.
    noise_floor_db: -86.0,
    // The early sidechain undershoots its markings.
    ratio_accuracy: 0.88,
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
    // The low noise circuit is what the LN in the name stands for.
    noise_floor_db: -96.0,
    ratio_accuracy: 1.0,
};

/// The Silverface: the same low noise front end as the D, with a push-pull
/// Class AB output stage in place of the Class A one.
pub const REV_F: Revision = Revision {
    name: "Rev F",
    slug: "comp76fx-rev-f",
    finish: Finish::SilverFace,
    stage: OutputStage::ClassAb,
    amp_drive: 0.34,
    fet_drive: 0.62,
    fet_bias: 0.04,
    noise_floor_db: -98.0,
    ratio_accuracy: 1.0,
};
