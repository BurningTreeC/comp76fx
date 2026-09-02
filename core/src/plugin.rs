//! The plugin body, shared by all three revisions.
//!
//! Everything except the identity is here. Each revision crate supplies its
//! name, its plugin identifiers and its [`Revision`] through the
//! [`export_revision!`] macro, so there is one implementation rather than
//! three copies that drift apart.

use nih_plug::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// What the front panel meter is fed, written by the audio thread and read by
/// the editor. Both are stored in hundredths of a dB so they fit an atomic.
#[derive(Default)]
pub struct Meters {
    /// Gain reduction, always positive.
    pub reduction: AtomicU32,
    /// Output level below full scale, as a positive number of dB down.
    pub output: AtomicU32,
}

impl Meters {
    pub fn reduction_db(&self) -> f32 {
        self.reduction.load(Ordering::Relaxed) as f32 / 100.0
    }

    /// Output level in dBFS, so negative.
    pub fn output_db(&self) -> f32 {
        -(self.output.load(Ordering::Relaxed) as f32 / 100.0)
    }
}

use crate::dsp::{Channel, Revision};
use crate::params::{Comp76Params, Oversampling};

/// Controls are refreshed at this granularity rather than per sample.
const CONTROL_BLOCK: usize = 32;

/// Latency the plugin always reports, whatever the oversampling setting is,
/// so that changing quality never renegotiates it while the host is running.
pub const LATENCY: u32 = 74;

/// Shared state of a Comp76Fx plugin.
pub struct Comp76 {
    pub params: Arc<Comp76Params>,
    revision: Revision,
    channels: Vec<Channel>,
    sample_rate: f32,
    oversampling: Oversampling,
    meters: Arc<Meters>,
}

impl Comp76 {
    pub fn new(revision: Revision, params: Arc<Comp76Params>) -> Self {
        Self {
            params,
            revision,
            channels: Vec::new(),
            sample_rate: 44100.0,
            oversampling: Oversampling::X4,
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
        self.channels.clear();
        self.channels.reserve(channels);
        for index in 0..channels {
            self.channels.push(Channel::new(
                self.revision,
                sample_rate as f64,
                self.oversampling.factor(),
                0x9e37_79b9_u32.wrapping_add(index as u32 * 0x85eb_ca6b),
            ));
        }
    }

    pub fn reset(&mut self) {
        self.channels.iter_mut().for_each(Channel::reset);
        self.meters.reduction.store(0, Ordering::Relaxed);
        self.meters.output.store(9999, Ordering::Relaxed);
    }

    pub fn process(&mut self, buffer: &mut Buffer) {
        let oversampling = self.params.oversampling.value();
        if oversampling != self.oversampling {
            self.oversampling = oversampling;
            for channel in self.channels.iter_mut() {
                channel.set_oversampling(oversampling.factor());
            }
        }

        // With the power off the unit is out of circuit entirely.
        if !self.params.power.value() {
            self.reset();
            return;
        }

        let mut peak = 0.0f32;
        for (_, mut block) in buffer.iter_blocks(CONTROL_BLOCK) {
            let steps = block.samples() as u32;
            let controls = self.params.controls(
                self.params.input.smoothed.next_step(steps),
                self.params.output.smoothed.next_step(steps),
                self.params.attack.smoothed.next_step(steps),
                self.params.release.smoothed.next_step(steps),
            );
            let mix = self.params.mix.smoothed.next_step(steps) / 100.0;

            for channel in self.channels.iter_mut() {
                channel.set_controls(controls);
            }

            for (index, samples) in block.iter_mut().enumerate() {
                let Some(channel) = self.channels.get_mut(index) else {
                    continue;
                };
                for sample in samples.iter_mut() {
                    let wet = channel.process(*sample);
                    let out = *sample * (1.0 - mix) + wet * mix;
                    *sample = out;
                    peak = peak.max(out.abs());
                }
            }
        }

        // The meter shows the deepest reduction any channel reached.
        let reduction = self
            .channels
            .iter_mut()
            .map(Channel::take_meter)
            .fold(0.0f32, f32::max);
        self.meters
            .reduction
            .store((reduction * 100.0) as u32, Ordering::Relaxed);
        // Stored as dB down so it stays a positive number.
        let output_db = 20.0 * (peak + 1e-9).log10();
        self.meters
            .output
            .store((-output_db * 100.0).clamp(0.0, 99999.0) as u32, Ordering::Relaxed);
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
            const CLAP_MANUAL_URL: Option<&'static str> = Some(<Self as ::nih_plug::prelude::Plugin>::URL);
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
