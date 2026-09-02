//! Comp76Fx Rev F, the Silverface.
//!
//! The same low noise front end as the D, but the Class A output stage has
//! given way to a push-pull Class AB one. It is cleaner and tighter, with the
//! distortion turning symmetrical, and it is the revision to reach for when
//! the D is too thick.

use comp76fx_core::dsp::{Finish, OutputStage, Revision};
use comp76fx_core::export_revision;

const REV_F: Revision = Revision {
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

export_revision! {
    name: "Comp76Fx Rev F",
    clap_id: "com.burningtreec.comp76fx.rev-f",
    vst3_id: b"Comp76Fx-RevF-01",
    description: "Blackface FET limiting amplifier with a push-pull output stage",
    revision: REV_F,
}
