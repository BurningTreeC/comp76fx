//! Comp76Fx Rev A, the Bluestripe.
//!
//! The original, before the low noise circuit existed. Its FET is run harder
//! and its Class A output stage is driven closer to its limits, so it is the
//! loudest, dirtiest and least accurate of the three: the ratios do not quite
//! reach their marked values, and the noise floor is audibly higher. That is
//! the sound people go looking for.
//!
//! The circuit values are [`comp76fx_core::dsp::REV_A`], kept in the core so
//! the tests measure exactly what ships.

use comp76fx_core::dsp::REV_A;
use comp76fx_core::export_revision;

export_revision! {
    name: "Comp76Fx Rev A",
    clap_id: "com.burningtreec.comp76fx.rev-a",
    vst3_id: b"Comp76Fx-RevA-01",
    description: "Bluestripe FET limiting amplifier, the original circuit",
    revision: REV_A,
}
