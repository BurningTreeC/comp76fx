//! Comp76Fx Rev D, the Blackface.
//!
//! The one most people mean by "an 1176": transformer input, Brad Plunkett's
//! low noise circuitry, and a Class A output stage. Universal Audio's own
//! reissue is patterned on the D and E versions, which are near enough
//! identical to each other.

use comp76fx_core::dsp::{Finish, OutputStage, Revision};
use comp76fx_core::export_revision;

const REV_D: Revision = Revision {
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

export_revision! {
    name: "Comp76Fx Rev D",
    clap_id: "com.burningtreec.comp76fx.rev-d",
    vst3_id: b"Comp76Fx-RevD-01",
    description: "Blackface FET limiting amplifier with low noise circuitry",
    revision: REV_D,
}
