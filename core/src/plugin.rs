//! The plugin body, shared by all three revisions.
//!
//! Everything except the identity is here. Each revision crate supplies its
//! name, its plugin identifiers and its [`Revision`] through the
//! [`export_revision!`] macro, so there is one implementation rather than
//! three copies that drift apart.

use nih_plug::prelude::*;
use std::sync::Arc;

use crate::dsp::{Channel, Controls, Delay, Revision};
use crate::meters::Meters;
use crate::params::{Comp76Params, Oversampling};

/// Controls are refreshed at this granularity rather than per sample.
const CONTROL_BLOCK: usize = 32;

/// Latency the plugin always reports, whatever the oversampling setting is,
/// so that changing quality never renegotiates it while the host is running.
/// Every channel is padded out to it, and the dry signal held back by it.
pub const LATENCY: u32 = crate::dsp::LATENCY;

/// One channel as the host sees it: the circuit, and the dry signal held back
/// to line up with it.
///
/// The circuit takes [`LATENCY`] samples to come out the other side, so a dry
/// signal blended in straight from the input arrived that much early and
/// combed against the wet one: with the blend at half and nothing being
/// compressed, 1 kHz came out 60 dB down at the default oversampling.
pub struct Strip {
    channel: Channel,
    dry: Delay,
}

impl Strip {
    pub fn new(revision: Revision, sample_rate: f64, factor: usize, seed: u32) -> Self {
        Self {
            channel: Channel::new(revision, sample_rate, factor, seed),
            dry: Delay::new(LATENCY as usize, LATENCY as usize),
        }
    }

    pub fn set_controls(&mut self, controls: Controls) {
        self.channel.set_controls(controls);
    }

    pub fn set_oversampling(&mut self, factor: usize) {
        self.channel.set_oversampling(factor);
    }

    /// One sample. `mix` is the share of the circuit in the output, from `0.0`
    /// to `1.0`. With the power off the unit is out of circuit and what comes
    /// out is the dry signal alone -- still held back, so switching it does
    /// not move the track in time against the rest of the session.
    #[inline]
    pub fn process(&mut self, sample: f32, mix: f32, powered: bool) -> f32 {
        let dry = self.dry.process(sample as f64) as f32;
        if !powered {
            return dry;
        }
        let wet = self.channel.process(sample);
        dry * (1.0 - mix) + wet * mix
    }

    /// Clears the circuit and leaves the dry signal running, as the power
    /// switch does.
    pub fn switch_off(&mut self) {
        self.channel.reset();
    }

    pub fn reset(&mut self) {
        self.channel.reset();
        self.dry.reset();
    }

    /// Gain reduction to show on the meter, in dB, and resets the peak hold.
    pub fn take_meter(&mut self) -> f32 {
        self.channel.take_meter()
    }
}

/// Shared state of a Comp76Fx plugin.
pub struct Comp76 {
    pub params: Arc<Comp76Params>,
    revision: Revision,
    strips: Vec<Strip>,
    /// Output energy per channel over the current buffer, for the meter.
    /// Allocated with the strips so the audio thread never has to.
    energy: Vec<f64>,
    sample_rate: f32,
    oversampling: Oversampling,
    /// Whether the power was on for the last buffer, so switching it off
    /// clears the circuit once rather than on every buffer.
    powered: bool,
    meters: Arc<Meters>,
}

impl Comp76 {
    pub fn new(revision: Revision, params: Arc<Comp76Params>) -> Self {
        Self {
            params,
            revision,
            strips: Vec::new(),
            energy: Vec::new(),
            sample_rate: 44100.0,
            oversampling: Oversampling::X4,
            powered: true,
            meters: Arc::new(Meters::default()),
        }
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn meters(&self) -> Arc<Meters> {
        self.meters.clone()
    }

    pub fn initialize(&mut self, channels: usize, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.oversampling = self.params.oversampling.value();
        self.strips = (0..channels)
            .map(|index| {
                Strip::new(
                    self.revision,
                    sample_rate as f64,
                    self.oversampling.factor(),
                    0x9e37_79b9_u32.wrapping_add(index as u32 * 0x85eb_ca6b),
                )
            })
            .collect();
        self.energy = vec![0.0; channels];
    }

    pub fn reset(&mut self) {
        self.strips.iter_mut().for_each(Strip::reset);
        self.meters.clear();
    }

    pub fn process(&mut self, buffer: &mut Buffer) {
        let oversampling = self.params.oversampling.value();
        if oversampling != self.oversampling {
            self.oversampling = oversampling;
            for strip in self.strips.iter_mut() {
                strip.set_oversampling(oversampling.factor());
            }
        }

        let powered = self.params.power.value();
        if !powered && self.powered {
            self.strips.iter_mut().for_each(Strip::switch_off);
        }
        self.powered = powered;

        self.energy.iter_mut().for_each(|energy| *energy = 0.0);
        for (_, mut block) in buffer.iter_blocks(CONTROL_BLOCK) {
            // The smoothers run whatever the power switch says, so a control
            // turned while it was off is where it was left when it comes on.
            let steps = block.samples() as u32;
            let controls = self.params.controls(
                self.params.input.smoothed.next_step(steps),
                self.params.output.smoothed.next_step(steps),
                self.params.attack.smoothed.next_step(steps),
                self.params.release.smoothed.next_step(steps),
            );
            let mix = self.params.mix.smoothed.next_step(steps) / 100.0;

            for (index, samples) in block.iter_mut().enumerate() {
                let (Some(strip), Some(energy)) =
                    (self.strips.get_mut(index), self.energy.get_mut(index))
                else {
                    continue;
                };
                strip.set_controls(controls);
                for sample in samples.iter_mut() {
                    let out = strip.process(*sample, mix, powered);
                    *sample = out;
                    *energy += out as f64 * out as f64;
                }
            }
        }

        if powered {
            // The meter shows the deepest reduction any channel reached, and
            // the level of the loudest one.
            let reduction = self
                .strips
                .iter_mut()
                .map(Strip::take_meter)
                .fold(0.0f32, f32::max);
            let loudest = self.energy.iter().copied().fold(0.0f64, f64::max);
            self.meters
                .publish(reduction, loudest as f32, buffer.samples() as u32);
        }
    }
}

/// Builds a plugin for one revision.
///
/// Each revision crate is only its identity: a name, the identifiers a host
/// uses to tell plugins apart, and the circuit differences.
#[macro_export]
macro_rules! export_revision {
    (
        name: $name:literal,
        clap_id: $clap_id:literal,
        vst3_id: $vst3_id:literal,
        description: $description:literal,
        revision: $revision:expr $(,)?
    ) => {
        pub struct Plugin76 {
            inner: $crate::plugin::Comp76,
        }

        impl Default for Plugin76 {
            fn default() -> Self {
                let params = ::std::sync::Arc::new($crate::params::Comp76Params::new(
                    $crate::editor::default_state(),
                ));
                Self {
                    inner: $crate::plugin::Comp76::new($revision, params),
                }
            }
        }

        impl ::nih_plug::prelude::Plugin for Plugin76 {
            const NAME: &'static str = $name;
            const VENDOR: &'static str = "BurningTreeC";
            const URL: &'static str = "https://github.com/BurningTreeC/comp76fx";
            const EMAIL: &'static str = "huber.simon@protonmail.com";
            const VERSION: &'static str = env!("CARGO_PKG_VERSION");

            const AUDIO_IO_LAYOUTS: &'static [::nih_plug::prelude::AudioIOLayout] = &[
                ::nih_plug::prelude::AudioIOLayout {
                    main_input_channels: ::nih_plug::prelude::NonZeroU32::new(2),
                    main_output_channels: ::nih_plug::prelude::NonZeroU32::new(2),
                    ..::nih_plug::prelude::AudioIOLayout::const_default()
                },
                ::nih_plug::prelude::AudioIOLayout {
                    main_input_channels: ::nih_plug::prelude::NonZeroU32::new(1),
                    main_output_channels: ::nih_plug::prelude::NonZeroU32::new(1),
                    ..::nih_plug::prelude::AudioIOLayout::const_default()
                },
            ];

            const SAMPLE_ACCURATE_AUTOMATION: bool = false;

            type SysExMessage = ();
            type BackgroundTask = ();

            fn params(&self) -> ::std::sync::Arc<dyn ::nih_plug::prelude::Params> {
                self.inner.params.clone()
            }

            fn editor(
                &mut self,
                _executor: ::nih_plug::prelude::AsyncExecutor<Self>,
            ) -> Option<Box<dyn ::nih_plug::prelude::Editor>> {
                $crate::editor::create(
                    self.inner.params.clone(),
                    self.inner.revision(),
                    self.inner.meters(),
                )
            }

            fn initialize(
                &mut self,
                layout: &::nih_plug::prelude::AudioIOLayout,
                config: &::nih_plug::prelude::BufferConfig,
                context: &mut impl ::nih_plug::prelude::InitContext<Self>,
            ) -> bool {
                let channels = layout
                    .main_output_channels
                    .map(::nih_plug::prelude::NonZeroU32::get)
                    .unwrap_or(2) as usize;
                self.inner.initialize(channels, config.sample_rate);
                context.set_latency_samples($crate::plugin::LATENCY);
                true
            }

            fn reset(&mut self) {
                self.inner.reset();
            }

            fn process(
                &mut self,
                buffer: &mut ::nih_plug::prelude::Buffer,
                _aux: &mut ::nih_plug::prelude::AuxiliaryBuffers,
                _context: &mut impl ::nih_plug::prelude::ProcessContext<Self>,
            ) -> ::nih_plug::prelude::ProcessStatus {
                self.inner.process(buffer);
                ::nih_plug::prelude::ProcessStatus::Normal
            }
        }

        impl ::nih_plug::prelude::ClapPlugin for Plugin76 {
            const CLAP_ID: &'static str = $clap_id;
            const CLAP_DESCRIPTION: Option<&'static str> = Some($description);
            const CLAP_MANUAL_URL: Option<&'static str> =
                Some(<Self as ::nih_plug::prelude::Plugin>::URL);
            const CLAP_SUPPORT_URL: Option<&'static str> = None;
            const CLAP_FEATURES: &'static [::nih_plug::prelude::ClapFeature] = &[
                ::nih_plug::prelude::ClapFeature::AudioEffect,
                ::nih_plug::prelude::ClapFeature::Stereo,
                ::nih_plug::prelude::ClapFeature::Mono,
                ::nih_plug::prelude::ClapFeature::Compressor,
                ::nih_plug::prelude::ClapFeature::Limiter,
            ];
        }

        impl ::nih_plug::prelude::Vst3Plugin for Plugin76 {
            const VST3_CLASS_ID: [u8; 16] = *$vst3_id;
            const VST3_SUBCATEGORIES: &'static [::nih_plug::prelude::Vst3SubCategory] = &[
                ::nih_plug::prelude::Vst3SubCategory::Fx,
                ::nih_plug::prelude::Vst3SubCategory::Dynamics,
            ];
        }

        ::nih_plug::nih_export_clap!(Plugin76);
        ::nih_plug::nih_export_vst3!(Plugin76);
    };
}
