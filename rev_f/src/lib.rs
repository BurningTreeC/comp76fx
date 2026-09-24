//! Comp76Fx Rev F, the Silverface.
//!
//! The same low noise front end as the D, but the Class A output stage has
//! given way to a push-pull Class AB one. It is cleaner and tighter, with the
//! distortion turning symmetrical, and it is the revision to reach for when
//! the D is too thick.
//!
//! The circuit values are [`comp76fx_core::dsp::REV_F`], kept in the core so
//! the tests measure exactly what ships.

use comp76fx_core::dsp::REV_F;
use comp76fx_core::export_revision;

export_revision! {
    name: "Comp76Fx Rev F",
    clap_id: "com.burningtreec.comp76fx.rev-f",
    vst3_id: b"Comp76Fx-RevF-01",
    description: "Silverface FET limiting amplifier with a push-pull output stage",
    revision: REV_F,
}
