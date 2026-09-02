//! Shared circuit model and front panel for the Comp76Fx limiting amplifiers.
//!
//! The three plugins built on this crate model three revisions of the same
//! 1966 solid state limiter. What they share is the signal path: a fixed
//! operating point that the input knob drives the signal against, a FET as the
//! gain element, a feedback sidechain, and an amplifier behind a transformer.
//! What differs is captured in [`dsp::Revision`].
//!
//! Copyright (C) 2026 Simon Huber
//!
//! This program is free software: you can redistribute it and/or modify it
//! under the terms of the GNU General Public License as published by the Free
//! Software Foundation, either version 3 of the License, or (at your option)
//! any later version. See `LICENSE` for details.

pub mod dsp;
pub mod editor;
pub mod params;
pub mod plugin;
pub mod presets;
