//! Front panel controls, shared by every revision.

use nih_plug::prelude::*;
use nih_plug_vizia::ViziaState;
use std::sync::{Arc, Mutex};

use crate::dsp::Controls;

/// What the meter is switched to. The hardware powers up with the meter
/// switch, so OFF is a real position rather than a hidden one.
#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum MeterMode {
    #[id = "gr"]
    #[name = "GR"]
    GainReduction,
    #[id = "plus4"]
    #[name = "+4"]
    Plus4,
    #[id = "plus8"]
    #[name = "+8"]
    Plus8,
    #[id = "off"]
    #[name = "Off"]
    Off,
}

#[derive(Enum, Debug, PartialEq, Eq, Clone, Copy)]
pub enum Oversampling {
    #[id = "1x"]
    #[name = "Off"]
    Off,
    #[id = "2x"]
    #[name = "2x"]
    X2,
    #[id = "4x"]
    #[name = "4x"]
    X4,
    #[id = "8x"]
    #[name = "8x"]
    X8,
}

impl Oversampling {
    pub fn factor(self) -> usize {
        match self {
            Oversampling::Off => 1,
            Oversampling::X2 => 2,
            Oversampling::X4 => 4,
            Oversampling::X8 => 8,
        }
    }
}

#[derive(Params)]
pub struct Comp76Params {
    #[persist = "editor-state"]
    pub editor_state: Arc<ViziaState>,
    #[persist = "preset"]
    pub preset_name: Arc<Mutex<String>>,

    /// How hard the signal is driven against the fixed operating point.
    #[id = "input"]
    pub input: FloatParam,
    /// Make-up gain after the gain element.
    #[id = "output"]
    pub output: FloatParam,
    /// Marked the way the panel is: fully clockwise is fastest.
    #[id = "attack"]
    pub attack: FloatParam,
    #[id = "release"]
    pub release: FloatParam,

    #[id = "ratio4"]
    pub ratio_4: BoolParam,
    #[id = "ratio8"]
    pub ratio_8: BoolParam,
    #[id = "ratio12"]
    pub ratio_12: BoolParam,
    #[id = "ratio20"]
    pub ratio_20: BoolParam,

    #[id = "meter"]
    pub meter: EnumParam<MeterMode>,
    #[id = "power"]
    pub power: BoolParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "os"]
    pub oversampling: EnumParam<Oversampling>,
}

fn switch(name: &'static str, default: bool) -> BoolParam {
    BoolParam::new(name, default)
        .with_value_to_string(Arc::new(|v| if v { "In" } else { "Out" }.to_string()))
        .with_string_to_value(Arc::new(|s| {
            Some(!matches!(
                s.trim().to_ascii_lowercase().as_str(),
                "out" | "off" | "false" | "no" | "0"
            ))
        }))
}

impl Comp76Params {
    /// The preset the panel was last set from, if any.
    pub fn preset_name(&self) -> String {
        self.preset_name
            .lock()
            .map(|name| name.clone())
            .unwrap_or_default()
    }

    pub fn set_preset_name(&self, name: &str) {
        if let Ok(mut current) = self.preset_name.lock() {
            name.clone_into(&mut current);
        }
    }

    pub fn new(editor_state: Arc<ViziaState>) -> Self {
        Self {
            editor_state,
            preset_name: Arc::new(Mutex::new(String::new())),

            input: FloatParam::new("Input", 0.0, FloatRange::Linear { min: -20.0, max: 40.0 })
                .with_unit(" dB")
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(formatters::v2s_f32_rounded(1)),
            output: FloatParam::new("Output", 0.0, FloatRange::Linear { min: -40.0, max: 20.0 })
                .with_unit(" dB")
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(formatters::v2s_f32_rounded(1)),
            // The panel is engraved 1 to 7, slowest to fastest.
            attack: dial("Attack", 0.5),
            release: dial("Release", 0.5),

            ratio_4: switch("Ratio 4:1", true),
            ratio_8: switch("Ratio 8:1", false),
            ratio_12: switch("Ratio 12:1", false),
            ratio_20: switch("Ratio 20:1", false),

            meter: EnumParam::new("Meter", MeterMode::GainReduction),
            power: BoolParam::new("Power", true)
                .with_value_to_string(Arc::new(|v| if v { "On" } else { "Off" }.to_string()))
                .with_string_to_value(Arc::new(|s| {
                    Some(!matches!(
                        s.trim().to_ascii_lowercase().as_str(),
                        "off" | "false" | "no" | "0"
                    ))
                })),
            mix: FloatParam::new("Mix", 100.0, FloatRange::Linear { min: 0.0, max: 100.0 })
                .with_unit(" %")
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_value_to_string(formatters::v2s_f32_rounded(0)),
            oversampling: EnumParam::new("Oversampling", Oversampling::X4),
        }
    }

    /// The panel as the circuit wants it.
    pub fn controls(&self, input: f32, output: f32, attack: f32, release: f32) -> Controls {
        Controls {
            input_db: input as f64,
            output_db: output as f64,
            attack: (attack / 7.0) as f64,
            release: (release / 7.0) as f64,
            buttons: [
                self.ratio_4.value(),
                self.ratio_8.value(),
                self.ratio_12.value(),
                self.ratio_20.value(),
            ],
        }
    }
}

/// The attack and release dials, engraved 1 to 7 like the hardware.
fn dial(name: &'static str, position: f32) -> FloatParam {
    FloatParam::new(
        name,
        1.0 + position * 6.0,
        FloatRange::Linear { min: 1.0, max: 7.0 },
    )
    .with_smoother(SmoothingStyle::Linear(20.0))
    .with_value_to_string(Arc::new(|v| format!("{v:.1}")))
    .with_string_to_value(Arc::new(|s| s.trim().parse().ok()))
}
