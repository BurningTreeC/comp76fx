//! The Comp76Fx front panel.
//!
//! One panel serves all three revisions. What changes between them is the
//! lettering and, on the earliest one, the painted section around the meter.

pub mod panel;
pub mod settings;
pub mod sprites;
pub mod style;
pub mod widgets;

use nih_plug::prelude::{Editor, Param};
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::{assets, create_vizia_editor, ViziaState, ViziaTheming};
use std::sync::Arc;

use crate::dsp::Revision;
use crate::params::Comp76Params;
use crate::plugin::Meters;
use panel::Faceplate;
use settings::{Dialogs, Header, SettingsOverlay, UiState};
use style::*;
use widgets::{Knob, PushButton, VuMeter};

/// Where everything sits on the panel.
pub mod layout {
    pub const INPUT_X: f32 = 128.0;
    pub const OUTPUT_X: f32 = 268.0;
    pub const ATTACK_X: f32 = 404.0;
    pub const RELEASE_X: f32 = 522.0;

    /// The ratio switches, left to right.
    pub const RATIO_X: [f32; 4] = [636.0, 686.0, 736.0, 786.0];
    pub const RATIO_W: f32 = 42.0;
    pub const RATIO_H: f32 = 58.0;

    pub const METER_X: f32 = 876.0;
    pub const METER_Y: f32 = 26.0;
    pub const METER_W: f32 = 214.0;
    pub const METER_H: f32 = 116.0;

    /// The meter switches, under the meter.
    pub const MODE_X: [f32; 4] = [886.0, 938.0, 990.0, 1042.0];
    pub const MODE_Y: f32 = 172.0;
    pub const MODE_W: f32 = 44.0;
    pub const MODE_H: f32 = 30.0;
}

#[derive(Lens)]
pub struct Panel {
    pub params: Arc<Comp76Params>,
}

impl Model for Panel {}

pub fn default_state() -> Arc<ViziaState> {
    ViziaState::new(|| (PANEL_W as u32, WINDOW_H as u32))
}

/// Height of a label box, which is centred on its anchor point.
const LABEL_H: f32 = 18.0;

pub fn create(
    params: Arc<Comp76Params>,
    revision: Revision,
    meters: Arc<Meters>,
) -> Option<Box<dyn Editor>> {
    let state = params.editor_state.clone();
    let state_for_scale = state.clone();
    create_vizia_editor(state, ViziaTheming::None, move |cx, _| {
        assets::register_noto_sans_regular(cx);
        assets::register_noto_sans_bold(cx);

        Panel {
            params: params.clone(),
        }
        .build(cx);
        UiState::new(
            state_for_scale.user_scale_factor(),
            params.clone(),
            revision.slug,
        )
        .build(cx);

        Header::new(cx);

        let params = params.clone();
        let meters = meters.clone();
        VStack::new(cx, move |cx| {
            faceplate(cx, revision, params.clone(), meters.clone());
        })
        .position_type(PositionType::SelfDirected)
        .left(Pixels(0.0))
        .top(Pixels(HEADER_H))
        .width(Pixels(PANEL_W))
        .height(Pixels(PANEL_H));

        SettingsOverlay::new(cx);
        Dialogs::new(cx);
    })
}

fn faceplate(cx: &mut Context, revision: Revision, params: Arc<Comp76Params>, meters: Arc<Meters>) {
    use layout::*;

    Faceplate::new(cx, revision);
    let ink = Ink::for_finish(revision.finish);

    // --- knobs --------------------------------------------------------------
    for (x, text, radius) in [
        (INPUT_X, "INPUT", R_LARGE),
        (OUTPUT_X, "OUTPUT", R_LARGE),
        (ATTACK_X, "ATTACK", R_SMALL),
        (RELEASE_X, "RELEASE", R_SMALL),
    ] {
        engraved(cx, ink, text, x, ROW + radius + 34.0, 11.0);
    }

    Knob::new(cx, Panel::params, |p| &p.input, R_LARGE).place(INPUT_X, ROW, R_LARGE);
    Knob::new(cx, Panel::params, |p| &p.output, R_LARGE).place(OUTPUT_X, ROW, R_LARGE);
    Knob::new(cx, Panel::params, |p| &p.attack, R_SMALL).place(ATTACK_X, ROW, R_SMALL);
    Knob::new(cx, Panel::params, |p| &p.release, R_SMALL).place(RELEASE_X, ROW, R_SMALL);

    // The attack and release dials are marked slowest to fastest, which is the
    // opposite way round from most compressors.
    for x in [ATTACK_X, RELEASE_X] {
        small(cx, ink, "SLOW", x - 40.0, ROW + 44.0, 7.5);
        small(cx, ink, "FAST", x + 40.0, ROW + 44.0, 7.5);
    }

    // --- ratio switches -----------------------------------------------------
    engraved(cx, ink, "RATIO", (RATIO_X[0] + RATIO_X[3]) / 2.0 + RATIO_W / 2.0, 46.0, 11.0);
    let ratio_ptrs: Vec<_> = {
        let p = &*params;
        vec![
            p.ratio_4.as_ptr(),
            p.ratio_8.as_ptr(),
            p.ratio_12.as_ptr(),
            p.ratio_20.as_ptr(),
        ]
    };
    let labels = ["4", "8", "12", "20"];
    for (index, x) in RATIO_X.iter().enumerate() {
        // Every other switch in the bank, for the mechanical interlock.
        let bank: Vec<_> = ratio_ptrs
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != index)
            .map(|(_, ptr)| *ptr)
            .collect();
        let button = match index {
            0 => PushButton::new(cx, Panel::params, |p| &p.ratio_4, labels[0], true, bank),
            1 => PushButton::new(cx, Panel::params, |p| &p.ratio_8, labels[1], true, bank),
            2 => PushButton::new(cx, Panel::params, |p| &p.ratio_12, labels[2], true, bank),
            _ => PushButton::new(cx, Panel::params, |p| &p.ratio_20, labels[3], true, bank),
        };
        button
            .position_type(PositionType::SelfDirected)
            .left(Pixels(*x))
            .top(Pixels(ROW - RATIO_H / 2.0))
            .width(Pixels(RATIO_W))
            .height(Pixels(RATIO_H));
        engraved(cx, ink, labels[index], x + RATIO_W / 2.0, ROW + RATIO_H / 2.0 + 16.0, 11.0);
    }
    small(
        cx,
        ink,
        "ALL FOUR IN FOR ALL-BUTTON MODE",
        (RATIO_X[0] + RATIO_X[3]) / 2.0 + RATIO_W / 2.0,
        ROW + RATIO_H / 2.0 + 36.0,
        8.0,
    );

    // --- meter --------------------------------------------------------------
    VuMeter::new(cx, meters, params.clone())
        .position_type(PositionType::SelfDirected)
        .left(Pixels(METER_X))
        .top(Pixels(METER_Y))
        .width(Pixels(METER_W))
        .height(Pixels(METER_H));

    let mode_labels = ["GR", "+4", "+8", "OFF"];
    for (index, x) in MODE_X.iter().enumerate() {
        widgets::ModeButton::new(cx, Panel::params, |p| &p.meter, index, 4)
            .position_type(PositionType::SelfDirected)
            .left(Pixels(*x))
            .top(Pixels(MODE_Y))
            .width(Pixels(MODE_W))
            .height(Pixels(MODE_H));
        small(cx, ink, mode_labels[index], x + MODE_W / 2.0, MODE_Y + MODE_H + 12.0, 9.0);
    }

    // --- nameplate ----------------------------------------------------------
    plate(cx, ink, "COMP76FX", 100.0, 28.0, 14.0);
    plate(cx, ink, "PEAK LIMITER", 100.0, 47.0, 9.0);
    plate(cx, ink, revision.name, 100.0, 65.0, 10.0);
    plate(cx, ink, "BURNINGTREEC", 100.0, 246.0, 8.0);
}

/// Extension for dropping a widget onto the panel at a centre point.
pub trait Place {
    fn place(self, x: f32, y: f32, radius: f32) -> Self;
}

impl<V: View> Place for Handle<'_, V> {
    fn place(self, x: f32, y: f32, radius: f32) -> Self {
        self.position_type(PositionType::SelfDirected)
            .left(Pixels(x - radius))
            .top(Pixels(y - radius))
    }
}

fn engraved(cx: &mut Context, ink: Ink, text: &str, x: f32, y: f32, size: f32) {
    let spaced = track_out(text);
    let width = size * spaced.chars().count() as f32 * 0.85 + 40.0;
    let (rr, rg, rb, ra) = ink.relief;
    let (tr, tg, tb) = ink.text;
    label_box(cx, &spaced, x, y + 1.0, size, width, rr, rg, rb, ra);
    label_box(cx, &spaced, x, y, size, width, tr, tg, tb, 255);
}

fn small(cx: &mut Context, ink: Ink, text: &str, x: f32, y: f32, size: f32) {
    let width = size * text.chars().count() as f32 * 0.72 + 10.0;
    let (rr, rg, rb, ra) = ink.relief;
    let (dr, dg, db) = ink.dim;
    label_box(cx, text, x, y + 1.0, size, width, rr, rg, rb, ra);
    label_box(cx, text, x, y, size, width, dr, dg, db, 255);
}

fn plate(cx: &mut Context, ink: Ink, text: &str, x: f32, y: f32, size: f32) {
    let spaced = track_out(text);
    let (rr, rg, rb, ra) = ink.relief;
    let (tr, tg, tb) = ink.text;
    for (dy, (r, g, b, a)) in [(1.0, (rr, rg, rb, ra)), (0.0, (tr, tg, tb, 255))] {
        Label::new(cx, &spaced)
            .position_type(PositionType::SelfDirected)
            .left(Pixels(x))
            .top(Pixels(y + dy - LABEL_H / 2.0))
            .width(Pixels(260.0))
            .height(Pixels(LABEL_H))
            .child_top(Stretch(1.0))
            .child_bottom(Stretch(1.0))
            .font_family(vec![FamilyOwned::Name(String::from(assets::NOTO_SANS))])
            .font_weight(FontWeightKeyword::Bold)
            .font_size(size)
            .color(Color::rgba(r, g, b, a));
    }
}

pub fn track_out(text: &str) -> String {
    text.chars()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join("\u{2009}")
}

#[allow(clippy::too_many_arguments)]
pub fn label_box(
    cx: &mut Context,
    text: &str,
    x: f32,
    y: f32,
    size: f32,
    width: f32,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
) {
    Label::new(cx, text)
        .position_type(PositionType::SelfDirected)
        .left(Pixels(x - width / 2.0))
        .top(Pixels(y - LABEL_H / 2.0))
        .width(Pixels(width))
        .height(Pixels(LABEL_H))
        .child_left(Stretch(1.0))
        .child_right(Stretch(1.0))
        .child_top(Stretch(1.0))
        .child_bottom(Stretch(1.0))
        .font_family(vec![FamilyOwned::Name(String::from(assets::NOTO_SANS))])
        .font_weight(FontWeightKeyword::Bold)
        .font_size(size)
        .color(Color::rgba(r, g, b, a))
        // Lettering is never the thing being clicked, and leaving it in the
        // way of the pointer breaks whatever it is drawn over. These labels
        // are positioned on top of the controls they annotate, and a later
        // sibling is the one the hit test finds -- events then travel up to
        // parents, never sideways to the control underneath. That is why the
        // oversampling switch could not be clicked at all.
        .hoverable(false);
}
