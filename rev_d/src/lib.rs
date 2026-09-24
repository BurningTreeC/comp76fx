//! Comp76Fx Rev D, the Blackface.
//!
//! The one most people mean by "an 1176": transformer input, Brad Plunkett's
//! low noise circuitry, and a Class A output stage. Universal Audio's own
//! reissue is patterned on the D and E versions, which are near enough
//! identical to each other.
//!
//! The circuit values are [`comp76fx_core::dsp::REV_D`], kept in the core so
//! the tests measure exactly what ships.

use comp76fx_core::dsp::REV_D;
use comp76fx_core::export_revision;

export_revision! {
    name: "Comp76Fx Rev D",
    clap_id: "com.burningtreec.comp76fx.rev-d",
    vst3_id: b"Comp76Fx-RevD-01",
    description: "Blackface FET limiting amplifier with low noise circuitry",
    revision: REV_D,
}
