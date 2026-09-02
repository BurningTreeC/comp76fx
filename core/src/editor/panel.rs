//! The faceplate: anodised aluminium, the painted section the Rev A is named
//! for, the rack hardware and the engraved scales around each control.

use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::vizia::vg;

use super::sprites::{self, Placement, Sprite};
use super::style::*;
use super::layout;
use crate::dsp::{Finish, Revision};

/// Fractions of the panel width the rack hardware sits at.
/// The mounting screws sit in the panel's corners. The right-hand column has
/// to clear the meter, which reaches x 1090 of 1120, so it goes outboard of it
/// rather than being lifted above it.
const SCREW_X: [f32; 2] = [0.0155, 0.9845];
const HARDWARE_Y: [f32; 2] = [0.125, 0.875];

pub struct Faceplate {
    /// Which panel this revision was built with.
    finish: Finish,
    /// One cache per screw; an image belongs to the canvas that uploaded it.
    screws: [Sprite; 4],
}

impl Faceplate {
    pub fn new(cx: &mut Context, revision: Revision) -> Handle<'_, Self> {
        Self {
            finish: revision.finish,
            screws: [Sprite::new(), Sprite::new(), Sprite::new(), Sprite::new()],
        }
        .build(cx, |_| {})
        .position_type(PositionType::SelfDirected)
        .left(Pixels(0.0))
        .top(Pixels(0.0))
        .width(Percentage(100.0))
        .height(Percentage(100.0))
    }
}

impl View for Faceplate {
    fn element(&self) -> Option<&'static str> {
        Some("comp76-faceplate")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let b = cx.bounds();
        let scale = cx.scale_factor();

        let silver = self.finish == Finish::SilverFace;
        let (top, bottom) = if silver {
            (SILVER_TOP, SILVER_BOTTOM)
        } else {
            (PANEL_TOP, PANEL_BOTTOM)
        };
        let mut panel = vg::Path::new();
        panel.rect(b.x, b.y, b.w, b.h);
        canvas.fill_path(
            &panel,
            &vg::Paint::linear_gradient(b.x, b.y, b.x, b.y + b.h, rgb(top), rgb(bottom)),
        );

        // Brushed aluminium runs along the panel, so the grain is horizontal.
        let lines = (b.h / (2.5 * scale)).max(1.0) as usize;
        for i in 0..lines {
            let y = b.y + (i as f32 + 0.5) * b.h / lines as f32;
            let shade = if i % 2 == 0 { 0x000000 } else { 0xffffff };
            // Bare metal shows its grain; paint over it mostly fills it in.
            let grain = if silver { 0.045 } else { 0.018 };
            let mut line = vg::Path::new();
            line.move_to(b.x, y);
            line.line_to(b.x + b.w, y);
            canvas.stroke_path(
                &line,
                &vg::Paint::color(rgba(shade, grain)).with_line_width(scale),
            );
        }

        // The painted section the Bluestripe is named after, around the meter.
        if self.finish == Finish::BlueStripe {
            // A band around the meter with bare panel either side of it,
            // rather than paint running off the end.
            let x = b.x + b.w * (layout::METER_X - 30.0) / PANEL_W;
            let w = b.w * (layout::METER_W + 50.0) / PANEL_W;
            let mut stripe = vg::Path::new();
            stripe.rect(x, b.y, w, b.h);
            canvas.fill_path(
                &stripe,
                &vg::Paint::linear_gradient(
                    x,
                    b.y,
                    x,
                    b.y + b.h,
                    rgba(BLUE_STRIPE, 0.92),
                    rgba(BLUE_STRIPE, 0.66),
                ),
            );
            for edge in [x, x + w] {
                let mut line = vg::Path::new();
                line.move_to(edge, b.y);
                line.line_to(edge, b.y + b.h);
                canvas.stroke_path(
                    &line,
                    &vg::Paint::color(rgba(0x000000, 0.4)).with_line_width(1.5 * scale),
                );
            }
        }

        // A pool of light towards the top.
        canvas.fill_path(
            &panel,
            &vg::Paint::radial_gradient(
                b.x + b.w * 0.4,
                b.y,
                0.0,
                b.h * 1.7,
                rgba(0xffffff, 0.07),
                rgba(0xffffff, 0.0),
            ),
        );
        // Falloff at the ends and the bottom.
        for (sx, sy, ex, ey, alpha) in [
            (b.x, b.y, b.x + b.w * 0.07, b.y, 0.30),
            (b.x + b.w, b.y, b.x + b.w * 0.93, b.y, 0.30),
            (b.x, b.y + b.h, b.x, b.y + b.h * 0.80, 0.32),
        ] {
            canvas.fill_path(
                &panel,
                &vg::Paint::linear_gradient(sx, sy, ex, ey, rgba(0x000000, alpha), rgba(0x000000, 0.0)),
            );
        }

        // Engraved scales around the knobs.
        let ink = Ink::for_finish(self.finish);
        let sx = b.w / PANEL_W;
        for (x, radius, divisions) in [
            (layout::INPUT_X, R_LARGE, 10),
            (layout::OUTPUT_X, R_LARGE, 10),
            (layout::ATTACK_X, R_SMALL, 6),
            (layout::RELEASE_X, R_SMALL, 6),
        ] {
            let cx0 = b.x + x * sx;
            let cy0 = b.y + ROW * sx;
            for i in 0..=divisions {
                let t = i as f32 / divisions as f32;
                draw_tick(
                    canvas,
                    cx0,
                    cy0,
                    (radius + 6.0) * sx,
                    (radius + 12.0) * sx,
                    knob_angle(t),
                    1.3 * scale,
                    ink,
                );
            }
        }

        // Rack hardware.
        for (i, &fx) in SCREW_X.iter().enumerate() {
            for (j, &fy) in HARDWARE_Y.iter().enumerate() {
                // No two screws are ever driven to the same angle, and each
                // photograph already carries its own.
                let k = i * 2 + j;
                self.screws[k].draw(
                    canvas,
                    sprites::SCREWS[k],
                    Placement {
                        x: b.x + b.w * fx,
                        y: b.y + b.h * fy,
                        height: 14.0 * scale,
                        degrees: 0.0,
                        pivot: sprites::CENTRE,
                    },
                );
            }
        }

        // Bevelled edges.
        let mut top = vg::Path::new();
        top.move_to(b.x, b.y + scale);
        top.line_to(b.x + b.w, b.y + scale);
        canvas.stroke_path(
            &top,
            &vg::Paint::color(rgba(0xffffff, 0.20)).with_line_width(scale * 2.0),
        );
        let mut bottom = vg::Path::new();
        bottom.move_to(b.x, b.y + b.h - scale);
        bottom.line_to(b.x + b.w, b.y + b.h - scale);
        canvas.stroke_path(
            &bottom,
            &vg::Paint::color(rgba(0x000000, 0.55)).with_line_width(scale * 2.5),
        );
    }
}
