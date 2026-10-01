//! The Comp76Fx front panel.
//!
//! One panel serves all three revisions. What changes between them is the
//! lettering and, on the earliest one, the painted section around the meter.

pub mod fonts;
pub mod paint;
pub mod panel;
pub mod settings;
pub mod sprites;
pub mod style;
pub mod widgets;

#[cfg(test)]
mod render_tests;

use nice_plug::prelude::Param;
use std::sync::Arc;
use vizia_plug::vizia::prelude::*;
use vizia_plug::{create_vizia_editor, ViziaState, ViziaTheming};

use crate::dsp::Revision;
use crate::meters::Meters;
use crate::params::Comp76Params;
use fonts::NOTO_SANS;
use panel::Faceplate;
use settings::{Dialogs, Header, SettingsOverlay, UiState};
use style::*;
use widgets::{Knob, PushButton, VuMeter};

/// The editor every revision's plugin hands its host.
pub type Editor = vizia_plug::ViziaEditor;

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

/// Physical pixels per unit of the panel's own layout at 100 %, as
/// GainStageFx draws its panel. Every size in the menu is relative to it, so
/// 200 % is 3.0, and the host's display scaling is not applied on top: the
/// panel is the same number of pixels on every display and in every host.
pub const BASE_DPI: f64 = 1.5;

/// The panel at 100 %, which renders at [`BASE_DPI`].
pub fn default_state() -> Arc<ViziaState> {
    ViziaState::new_with_base_scale_factor(|| (PANEL_W as u32, WINDOW_H as u32), BASE_DPI)
}

/// Updates the scale used by `Editor::size()` and saved in the host session.
/// `PersistentField::set` copies the carrier's scale; the original state's
/// size function and open status stay intact.
///
/// Called once the window has actually been resized to it (see
/// `UiEvent::SetScale`). The Vizia adapter records the same figure itself
/// when the native resize settles; this is the panel saying so as well.
pub fn remember_scale(state: &Arc<ViziaState>, scale: f64) {
    use nice_plug::params::persist::PersistentField;
    let carrier = ViziaState::new_with_default_scale_factor(|| (0, 0), scale);
    if let Ok(carrier) = Arc::try_unwrap(carrier) {
        PersistentField::set(state, carrier);
    }
}

/// Height of a label box, which is centred on its anchor point.
const LABEL_H: f32 = 18.0;

pub fn create(
    params: Arc<Comp76Params>,
    revision: Revision,
    meters: Arc<Meters>,
) -> Option<Editor> {
    let state = params.editor_state.clone();
    create_vizia_editor(state, ViziaTheming::None, move |cx, _gui| {
        build(cx, params.clone(), revision, meters.clone());
    })
}

/// The whole panel, into a context the editor or a test has set up.
pub fn build(cx: &mut Context, params: Arc<Comp76Params>, revision: Revision, meters: Arc<Meters>) {
    fonts::register_noto_sans(cx);
    // The only styling the panel takes from a sheet rather than from its
    // own drawing. See `style::STYLESHEET` for why it cannot be inline.
    let _ = cx.add_stylesheet(STYLESHEET);

    UiState::new(
        params.editor_state.user_scale_factor(),
        params.clone(),
        revision,
    )
    .build(cx);

    Header::new(cx);

    VStack::new(cx, move |cx| {
        faceplate(cx, revision, params.clone(), meters.clone());
    })
    .position_type(PositionType::Absolute)
    .left(Pixels(0.0))
    .top(Pixels(HEADER_H))
    .width(Pixels(PANEL_W))
    .height(Pixels(PANEL_H));

    SettingsOverlay::new(cx);
    Dialogs::new(cx);
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

    Knob::new(cx, &params.input, R_LARGE).place(INPUT_X, ROW, R_LARGE);
    Knob::new(cx, &params.output, R_LARGE).place(OUTPUT_X, ROW, R_LARGE);
    // The attack control carries the limiting switch at its anticlockwise end.
    Knob::with_off_switch(cx, &params.attack, &params.limiting, R_SMALL)
        .place(ATTACK_X, ROW, R_SMALL);
    Knob::new(cx, &params.release, R_SMALL).place(RELEASE_X, ROW, R_SMALL);

    // The attack and release dials are marked slowest to fastest, which is the
    // opposite way round from most compressors. The attack dial's slow end is
    // the OFF switch, which disables the limiting.
    for (x, slow) in [(ATTACK_X, "OFF"), (RELEASE_X, "SLOW")] {
        small(cx, ink, slow, x - 40.0, ROW + 44.0, 7.5);
        small(cx, ink, "FAST", x + 40.0, ROW + 44.0, 7.5);
    }

    // --- ratio switches -----------------------------------------------------
    engraved(
        cx,
        ink,
        "RATIO",
        (RATIO_X[0] + RATIO_X[3]) / 2.0 + RATIO_W / 2.0,
        46.0,
        11.0,
    );
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
            0 => PushButton::new(cx, &params.ratio_4, bank),
            1 => PushButton::new(cx, &params.ratio_8, bank),
            2 => PushButton::new(cx, &params.ratio_12, bank),
            _ => PushButton::new(cx, &params.ratio_20, bank),
        };
        button
            .position_type(PositionType::Absolute)
            .left(Pixels(*x))
            .top(Pixels(ROW - RATIO_H / 2.0))
            .width(Pixels(RATIO_W))
            .height(Pixels(RATIO_H));
        engraved(
            cx,
            ink,
            labels[index],
            x + RATIO_W / 2.0,
            ROW + RATIO_H / 2.0 + 16.0,
            11.0,
        );
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
        .position_type(PositionType::Absolute)
        .left(Pixels(METER_X))
        .top(Pixels(METER_Y))
        .width(Pixels(METER_W))
        .height(Pixels(METER_H));

    let mode_labels = ["GR", "+4", "+8", "OFF"];
    for (index, x) in MODE_X.iter().enumerate() {
        widgets::ModeButton::new(cx, &params.meter, index, 4)
            .position_type(PositionType::Absolute)
            .left(Pixels(*x))
            .top(Pixels(MODE_Y))
            .width(Pixels(MODE_W))
            .height(Pixels(MODE_H));
        small(
            cx,
            ink,
            mode_labels[index],
            x + MODE_W / 2.0,
            MODE_Y + MODE_H + 12.0,
            9.0,
        );
    }

    // --- nameplate ----------------------------------------------------------
    plate(cx, ink, "COMP76FX", 100.0, 26.0, 14.0);
    // The version, under the name. Small and in the relief ink like the rest
    // of the plate: it is there to be quoted when reporting a fault, not read
    // every session.
    plate(
        cx,
        ink,
        concat!("V", env!("CARGO_PKG_VERSION")),
        100.0,
        40.0,
        7.5,
    );
    plate(cx, ink, "PEAK LIMITER", 100.0, 54.0, 9.0);
    plate(cx, ink, revision.name, 100.0, 71.0, 10.0);
    plate(cx, ink, "BURNINGTREEC", 100.0, 246.0, 8.0);
}

/// Extension for dropping a widget onto the panel at a centre point.
pub trait Place {
    fn place(self, x: f32, y: f32, radius: f32) -> Self;
}

impl<V: View> Place for Handle<'_, V> {
    fn place(self, x: f32, y: f32, radius: f32) -> Self {
        self.position_type(PositionType::Absolute)
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
        Label::new(cx, spaced.clone())
            .position_type(PositionType::Absolute)
            .left(Pixels(x))
            .top(Pixels(y + dy - LABEL_H / 2.0))
            .width(Pixels(260.0))
            .height(Pixels(LABEL_H))
            .alignment(Alignment::Left)
            .font_family(noto_sans())
            .font_weight(FontWeightKeyword::Bold)
            .font_size(size)
            .color(Color::rgba(r, g, b, a))
            .hoverable(false);
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
    Label::new(cx, text.to_owned())
        .position_type(PositionType::Absolute)
        .left(Pixels(x - width / 2.0))
        .top(Pixels(y - LABEL_H / 2.0))
        .width(Pixels(width))
        .height(Pixels(LABEL_H))
        .alignment(Alignment::Center)
        .text_align(TextAlign::Center)
        .font_family(noto_sans())
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

/// The panel's typeface, as a view's font family.
pub fn noto_sans() -> Vec<FamilyOwned> {
    vec![FamilyOwned::Named(String::from(NOTO_SANS))]
}
