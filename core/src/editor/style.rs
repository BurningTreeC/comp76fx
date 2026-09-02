//! Panel colours, geometry and the drawing primitives the widgets share.

use crate::dsp::Finish;
use nih_plug_vizia::vizia::prelude::{BoundingBox, Canvas};
use nih_plug_vizia::vizia::vg;

/// The unit is a two rack unit panel; this is a little taller than that so the
/// lettering stays readable.
pub const PANEL_W: f32 = 1120.0;
pub const PANEL_H: f32 = 260.0;
/// The strip above the panel carrying the presets and settings.
pub const HEADER_H: f32 = 34.0;
pub const WINDOW_H: f32 = PANEL_H + HEADER_H;

/// Centre line of the row of knobs and buttons.
pub const ROW: f32 = 130.0;

pub const R_LARGE: f32 = 42.0;
pub const R_SMALL: f32 = 34.0;
/// Where the engraved scale sits around a knob.
pub const SCALE_RADIUS: f32 = 56.0;
/// A knob sweeps this many degrees, zero at the lower left.
pub const SWEEP: f32 = 300.0;

/// The black panels: Rev A and B under the painted band, and the low noise
/// Rev C to E throughout.
pub const PANEL_TOP: u32 = 0x2a2c2e;
pub const PANEL_BOTTOM: u32 = 0x141516;
/// The painted section around the meter that gives the Rev A its nickname.
pub const BLUE_STRIPE: u32 = 0x1d4f7a;

/// The bare brushed aluminium of the UREI era panels, which were never
/// painted. Much lighter than the black ones, so the lettering on them is
/// black rather than white.
pub const SILVER_TOP: u32 = 0xc4c8cc;
pub const SILVER_BOTTOM: u32 = 0x8d9297;

/// How lettering sits on a faceplate: the ink itself, a dimmer ink for the
/// small scale markings, and the offset copy underneath that gives the
/// letters relief. On a black panel the relief is a shadow; on aluminium it
/// is a highlight, because the light is coming from the same place but the
/// surface it falls on is now brighter than the ink.
#[derive(Clone, Copy)]
pub struct Ink {
    pub text: (u8, u8, u8),
    pub dim: (u8, u8, u8),
    pub relief: (u8, u8, u8, u8),
}

impl Ink {
    pub fn for_finish(finish: Finish) -> Self {
        match finish {
            Finish::SilverFace => Ink {
                text: (0x1c, 0x1e, 0x20),
                dim: (0x3e, 0x42, 0x46),
                relief: (0xff, 0xff, 0xff, 130),
            },
            Finish::BlueStripe | Finish::BlackFace => Ink {
                text: (0xe8, 0xea, 0xee),
                dim: (0xb4, 0xba, 0xc2),
                relief: (0, 0, 0, 150),
            },
        }
    }
}

pub fn rgb(hex: u32) -> vg::Color {
    vg::Color::rgb(
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
    )
}

pub fn rgba(hex: u32, alpha: f32) -> vg::Color {
    let mut c = rgb(hex);
    c.set_alphaf(alpha);
    c
}

/// Position on a circle, angles measured clockwise from twelve o'clock.
pub fn polar(cx: f32, cy: f32, radius: f32, degrees: f32) -> (f32, f32) {
    let a = degrees.to_radians();
    (cx + radius * a.sin(), cy - radius * a.cos())
}

pub fn knob_angle(normalized: f32) -> f32 {
    (normalized - 0.5) * SWEEP
}

/// The shadow a control casts on the panel. Every control sits in the same
/// light, so the drawn ones and the rendered sprites share this rather than
/// each carrying a shadow of its own.
pub fn contact_shadow(canvas: &mut Canvas, cx: f32, cy: f32, r: f32) {
    let mut path = vg::Path::new();
    path.ellipse(cx, cy + r * 0.16, r * 1.18, r * 1.12);
    canvas.fill_path(
        &path,
        &vg::Paint::radial_gradient(
            cx,
            cy + r * 0.16,
            r * 0.72,
            r * 1.18,
            rgba(0x000000, 0.55),
            rgba(0x000000, 0.0),
        ),
    );
}

/// The shadow a rectangular part casts on the panel: a button bezel or the
/// meter's case. The offset is downward because the light is above, and the
/// feather is what stops it reading as a drawn outline.
pub fn cast_shadow(canvas: &mut Canvas, b: BoundingBox, scale: f32, spread: f32, radius: f32) {
    let drop = spread * 0.45;
    let mut path = vg::Path::new();
    path.rounded_rect(
        b.x - spread,
        b.y - spread + drop,
        b.w + spread * 2.0,
        b.h + spread * 2.0,
        radius + spread,
    );
    canvas.fill_path(
        &path,
        &vg::Paint::box_gradient(
            b.x,
            b.y + drop,
            b.w,
            b.h,
            radius,
            spread * scale.max(1.0),
            rgba(0x000000, 0.55),
            rgba(0x000000, 0.0),
        ),
    );
}

/// One of the black pointer knobs. Finer knurling than the Pultec's, a domed
/// top and a single white index line.
pub fn draw_knob(canvas: &mut Canvas, cx: f32, cy: f32, r: f32, angle: f32) {
    let rot = angle.to_radians();
    contact_shadow(canvas, cx, cy, r);

    // Body, knurled around the rim.
    const TEETH: usize = 34;
    const STEPS: usize = TEETH * 6;
    let mut body = vg::Path::new();
    for i in 0..=STEPS {
        let t = i as f32 / STEPS as f32 * std::f32::consts::TAU;
        let knurl = 1.0 - 0.018 * (1.0 - (t * TEETH as f32).cos());
        let a = t + rot;
        let (x, y) = (cx + r * knurl * a.sin(), cy - r * knurl * a.cos());
        if i == 0 {
            body.move_to(x, y);
        } else {
            body.line_to(x, y);
        }
    }
    body.close();
    canvas.fill_path(
        &body,
        &vg::Paint::linear_gradient(cx, cy - r, cx, cy + r, rgb(0x33_33_37), rgb(0x08_08_0a)),
    );
    canvas.fill_path(
        &body,
        &vg::Paint::radial_gradient(cx, cy, r * 0.6, r, rgba(0x000000, 0.0), rgba(0x000000, 0.6)),
    );
    // Rim light along the top edge.
    canvas.stroke_path(
        &body,
        &vg::Paint::linear_gradient(
            cx,
            cy - r,
            cx,
            cy + r * 0.4,
            rgba(0xd4dae0, 0.45),
            rgba(0xd4dae0, 0.0),
        )
        .with_line_width(r * 0.05),
    );

    // Domed top.
    let top = r * 0.70;
    let mut face = vg::Path::new();
    face.circle(cx, cy, top);
    canvas.fill_path(
        &face,
        &vg::Paint::radial_gradient(
            cx - top * 0.35,
            cy - top * 0.42,
            top * 0.05,
            top * 1.6,
            rgb(0x45_45_4a),
            rgb(0x0b_0b_0d),
        ),
    );
    canvas.stroke_path(
        &face,
        &vg::Paint::color(rgba(0x000000, 0.5)).with_line_width(r * 0.04),
    );
    let mut sheen = vg::Path::new();
    sheen.ellipse(cx - top * 0.28, cy - top * 0.40, top * 0.44, top * 0.24);
    canvas.fill_path(
        &sheen,
        &vg::Paint::radial_gradient(
            cx - top * 0.28,
            cy - top * 0.40,
            0.0,
            top * 0.48,
            rgba(0xff_ff_ff, 0.20),
            rgba(0xff_ff_ff, 0.0),
        ),
    );

    // The index line, running from the middle out across the rim.
    let (sa, ca) = rot.sin_cos();
    let mut index = vg::Path::new();
    index.move_to(cx + r * 0.12 * sa, cy - r * 0.12 * ca);
    index.line_to(cx + r * 0.94 * sa, cy - r * 0.94 * ca);
    canvas.stroke_path(
        &index,
        &vg::Paint::color(rgba(0x000000, 0.7)).with_line_width(r * 0.14),
    );
    canvas.stroke_path(
        &index,
        &vg::Paint::color(rgb(0xf2_f0_ea)).with_line_width(r * 0.075),
    );
}

/// A tick engraved into the panel around a knob.
#[allow(clippy::too_many_arguments)]
pub fn draw_tick(
    canvas: &mut Canvas,
    cx: f32,
    cy: f32,
    inner: f32,
    outer: f32,
    degrees: f32,
    width: f32,
    ink: Ink,
) {
    let (x0, y0) = polar(cx, cy, inner, degrees);
    let (x1, y1) = polar(cx, cy, outer, degrees);
    let mut path = vg::Path::new();
    path.move_to(x0, y0);
    path.line_to(x1, y1);
    // The wide pass underneath is the relief, the narrow one the mark itself,
    // so a scale cut into aluminium reads the same way as one screened onto
    // black: dark line, light edge, or the other way round.
    let (rr, rg, rb, ra) = ink.relief;
    let mut under = vg::Color::rgb(rr, rg, rb);
    under.set_alphaf(ra as f32 / 255.0 * 0.85);
    canvas.stroke_path(&path, &vg::Paint::color(under).with_line_width(width * 2.0));
    let (tr, tg, tb) = ink.text;
    let mut over = vg::Color::rgb(tr, tg, tb);
    over.set_alphaf(0.9);
    canvas.stroke_path(&path, &vg::Paint::color(over).with_line_width(width));
}

