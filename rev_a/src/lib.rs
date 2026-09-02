//! Comp76Fx Rev A, the Bluestripe.
//!
//! The original, before the low noise circuit existed. Its FET is run harder
//! and its Class A output stage is driven closer to its limits, so it is the
//! loudest, dirtiest and least accurate of the three: the ratios do not quite
//! reach their marked values, and the noise floor is audibly higher. That is
//! the sound people go looking for.

use comp76fx_core::dsp::{Finish, OutputStage, Revision};
use comp76fx_core::export_revision;

const REV_A: Revision = Revision {
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

export_revision! {
    name: "Comp76Fx Rev A",
    clap_id: "com.burningtreec.comp76fx.rev-a",
    vst3_id: b"Comp76Fx-RevA-01",
    description: "Bluestripe FET limiting amplifier, the original circuit",
    revision: REV_A,
}
