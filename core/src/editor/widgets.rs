//! The panel's controls: knobs, the latching push buttons and the meter.

use nih_plug::prelude::Param;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::vizia::vg;
use nih_plug_vizia::widgets::param_base::ParamWidgetBase;
use nih_plug_vizia::widgets::{util::ModifiersExt, RawParamEvent};
use std::cell::Cell;
use std::sync::Arc;
use std::time::Duration;

use super::sprites::{self, Placement, Sprite};
use super::style::*;
use crate::params::{Comp76Params, MeterMode};
use crate::plugin::Meters;

/// Pixels of vertical drag for the full range of a knob.
const DRAG_RANGE: f32 = 240.0;
/// How much finer the drag becomes while shift is held.
const FINE: f32 = 0.15;

// ---------------------------------------------------------------------------
// Knob
// ---------------------------------------------------------------------------

pub struct Knob {
    param: ParamWidgetBase,
    radius: f32,
    dragging: bool,
    last_y: f32,
    face: Sprite,
}

impl Knob {
    pub fn new<L, Params, P, FMap>(
        cx: &mut Context,
        params: L,
        params_to_param: FMap,
        radius: f32,
    ) -> Handle<'_, Self>
    where
        L: Lens<Target = Params> + Clone,
        Params: 'static,
        P: Param + 'static,
        FMap: Fn(&Params) -> &P + Copy + 'static,
    {
        Self {
            param: ParamWidgetBase::new(cx, params, params_to_param),
            radius,
            dragging: false,
            last_y: 0.0,
            face: Sprite::new(),
        }
        .build(
            cx,
            ParamWidgetBase::build_view(params, params_to_param, move |cx, data| {
                let value = data.make_lens(|param| param.modulated_normalized_value());
                Binding::new(cx, value, |cx, _| cx.needs_redraw());
            }),
        )
        .width(Pixels(radius * 2.0))
        .height(Pixels(radius * 2.0))
    }

    fn nudge(&self, cx: &mut EventContext, delta: f32) {
        let current = self.param.unmodulated_normalized_value();
        self.param
            .set_normalized_value(cx, (current + delta).clamp(0.0, 1.0));
    }

    /// Ends a drag: releases the mouse and closes the gesture with the host.
    ///
    /// Called from more than one place because the one that must not be relied
    /// on is the mouse button coming back up. See `event`.
    fn finish(&mut self, cx: &mut EventContext) {
        if !self.dragging {
            return;
        }
        self.dragging = false;
        cx.release();
        cx.set_active(false);
        self.param.end_set_parameter(cx);
    }
}

impl View for Knob {
    fn element(&self) -> Option<&'static str> {
        Some("comp76-knob")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let bounds = cx.bounds();
        let r = self.radius * cx.scale_factor();
        // The panel knobs and the small trim knobs are different castings.
        const TRIM_BELOW: f32 = 25.0;
        let (bytes, rest) = if self.radius >= TRIM_BELOW {
            (sprites::KNOB_LARGE, sprites::KNOB_LARGE_REST)
        } else {
            (sprites::KNOB_SMALL, sprites::KNOB_SMALL_REST)
        };
        // Pick the frame rendered at this angle rather than turning one image,
        // which would carry the lighting round with the knob.
        let position = self.param.modulated_normalized_value().clamp(0.0, 1.0);
        let frame = (position * (sprites::KNOB_FRAMES - 1) as f32).round() as usize;
        let _ = rest;
        // The render is framed to the body's silhouette and stops dead at its
        // edge, so the knob has to be given the same contact shadow the drawn
        // controls lay down or it sits on the panel with nothing under it.
        contact_shadow(
            canvas,
            bounds.x + bounds.w / 2.0,
            bounds.y + bounds.h / 2.0,
            r * 1.2,
        );
        self.face.draw_frame(
            canvas,
            bytes,
            Placement {
                x: bounds.x + bounds.w / 2.0,
                y: bounds.y + bounds.h / 2.0,
                height: r * 2.4,
                degrees: 0.0,
                pivot: sprites::CENTRE,
            },
            frame,
            sprites::KNOB_FRAMES,
        );
    }

    /// Mouse handling, and the one thing in it that is not obvious.
    ///
    /// A drag captures the mouse so that the control keeps receiving movement
    /// when the pointer leaves it, and releases on the button coming back up.
    /// That release must not be the *only* way out.
    ///
    /// vizia routes every mouse event to the captured entity, and nothing in
    /// vizia ever clears a capture on its own -- `MouseCaptureOutEvent` is
    /// declared in its event enum and emitted nowhere, and `release` only
    /// clears the field when the widget itself asks. So a drag whose button-up
    /// never arrives leaves this control holding the mouse for the rest of the
    /// session: every other control stops responding, the window looks frozen,
    /// and the audio thread carries on as though nothing were wrong. The
    /// gesture opened with the host is never closed either, so it also thinks
    /// an edit is still in progress.
    ///
    /// A button-up can genuinely go missing. On Windows the pointer is held
    /// with `SetCapture`, and a `WM_CAPTURECHANGED` -- another window taking
    /// capture, the host putting up a dialog, the plugin window being
    /// deactivated mid-drag -- sends the button-up somewhere else entirely.
    ///
    /// So the drag is also ended by anything that says the mouse is no longer
    /// down, and the check that does not depend on an event arriving at all is
    /// in `MouseMove`: if the button is up while this control thinks it is
    /// dragging, the drag is over whether or not anyone said so. That one
    /// heals the window the moment the pointer moves over it again.
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|param_event, _| {
            if let RawParamEvent::ParametersChanged = param_event {
                cx.needs_redraw();
            }
        });
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left)
            | WindowEvent::MouseTripleClick(MouseButton::Left) => {
                if cx.modifiers().command() {
                    self.param.begin_set_parameter(cx);
                    self.param
                        .set_normalized_value(cx, self.param.default_normalized_value());
                    self.param.end_set_parameter(cx);
                } else if !self.dragging {
                    // Guarded: a second press without an intervening release
                    // would open a gesture inside a gesture, which is not
                    // something a host has to make sense of.
                    self.dragging = true;
                    self.last_y = cx.mouse().cursory;
                    cx.capture();
                    cx.focus();
                    cx.set_active(true);
                    self.param.begin_set_parameter(cx);
                }
                meta.consume();
            }
            WindowEvent::MouseDoubleClick(MouseButton::Left)
            | WindowEvent::MouseDown(MouseButton::Right) => {
                self.param.begin_set_parameter(cx);
                self.param
                    .set_normalized_value(cx, self.param.default_normalized_value());
                self.param.end_set_parameter(cx);
                meta.consume();
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if self.dragging {
                    self.finish(cx);
                    meta.consume();
                }
            }
            // Anything that means this window is no longer the one being used.
            // These are the events that do arrive when a drag is interrupted;
            // the check in `MouseMove` covers the times none of them does.
            WindowEvent::FocusOut
            | WindowEvent::WindowClose
            | WindowEvent::MouseCaptureOutEvent => {
                self.finish(cx);
            }
            WindowEvent::MouseMove(_, y) => {
                if self.dragging {
                    // The button came up somewhere this window never heard
                    // about. Without this the control holds the mouse for good.
                    if cx.mouse().left.state == MouseButtonState::Released {
                        self.finish(cx);
                        return;
                    }
                    let speed = if cx.modifiers().shift() { FINE } else { 1.0 };
                    let delta = (self.last_y - *y) / (DRAG_RANGE * cx.scale_factor()) * speed;
                    self.last_y = *y;
                    self.nudge(cx, delta);
                    cx.needs_redraw();
                }
            }
            WindowEvent::MouseScroll(_, y) => {
                let step = if cx.modifiers().shift() { 0.005 } else { 0.02 };
                self.param.begin_set_parameter(cx);
                self.nudge(cx, y * step);
                self.param.end_set_parameter(cx);
                cx.needs_redraw();
                meta.consume();
            }
            _ => {}
        });
    }
}

// ---------------------------------------------------------------------------
// Push buttons
// ---------------------------------------------------------------------------

/// A latching push button, as the ratio and meter switches are.
///
/// The ratio switches on the hardware are mechanically interlocked, so pressing
/// one releases the others, but they can all be pushed in together if you are
/// quick or determined. Here, a plain click behaves like the interlock and a
/// modified click latches, which is how you get all-button mode without having
/// to be quick.
pub struct PushButton {
    param: ParamWidgetBase,
    cap: Sprite,
    label: &'static str,
    /// Pressing this one alone releases its neighbours.
    interlocked: bool,
    /// The other switches in the same bank, for the interlock.
    bank: Vec<nih_plug::prelude::ParamPtr>,
}

impl PushButton {
    pub fn new<'a, L, Params, P, FMap>(
        cx: &'a mut Context,
        params: L,
        params_to_param: FMap,
        label: &'static str,
        interlocked: bool,
        bank: Vec<nih_plug::prelude::ParamPtr>,
    ) -> Handle<'a, Self>
    where
        L: Lens<Target = Params> + Clone,
        Params: 'static,
        P: Param + 'static,
        FMap: Fn(&Params) -> &P + Copy + 'static,
    {
        Self {
            param: ParamWidgetBase::new(cx, params, params_to_param),
            cap: Sprite::new(),
            label,
            interlocked,
            bank,
        }
        .build(
            cx,
            ParamWidgetBase::build_view(params, params_to_param, move |cx, data| {
                let value = data.make_lens(|param| param.modulated_normalized_value());
                Binding::new(cx, value, |cx, _| cx.needs_redraw());
            }),
        )
    }
}

impl View for PushButton {
    fn element(&self) -> Option<&'static str> {
        Some("comp76-button")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let pressed = self.param.modulated_normalized_value() > 0.5;
        self.cap
            .draw_button(canvas, cx.bounds(), cx.scale_factor(), pressed);
        let _ = self.label;
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|param_event, _| {
            if let RawParamEvent::ParametersChanged = param_event {
                cx.needs_redraw();
            }
        });
        event.map(|window_event, meta| {
            if let WindowEvent::MouseDown(MouseButton::Left) = window_event {
                let was = self.param.modulated_normalized_value() > 0.5;
                // Shift or ctrl latches, which is how all four go in at once.
                let latching = cx.modifiers().shift() || cx.modifiers().command();

                if self.interlocked && !latching {
                    // Release the rest of the bank, the way the mechanical
                    // interlock does.
                    for &other in &self.bank {
                        cx.emit(RawParamEvent::BeginSetParameter(other));
                        cx.emit(RawParamEvent::SetParameterNormalized(other, 0.0));
                        cx.emit(RawParamEvent::EndSetParameter(other));
                    }
                    self.param.begin_set_parameter(cx);
                    self.param.set_normalized_value(cx, 1.0);
                    self.param.end_set_parameter(cx);
                } else {
                    self.param.begin_set_parameter(cx);
                    self.param
                        .set_normalized_value(cx, if was { 0.0 } else { 1.0 });
                    self.param.end_set_parameter(cx);
                }
                cx.needs_redraw();
                meta.consume();
            }
        });
    }
}

// ---------------------------------------------------------------------------
// The meter
// ---------------------------------------------------------------------------

/// The moving coil meter.
///
/// A real VU movement takes about 300 ms to settle, and that lag is a large
/// part of how one reads. The needle is smoothed towards its target rather
/// than snapped to it, with a little overshoot so it does not feel dead.
pub struct VuMeter {
    face: Sprite,
    meters: Arc<Meters>,
    params: Arc<Comp76Params>,
    /// Where the needle actually is, and how fast it is travelling.
    position: Cell<f32>,
    velocity: Cell<f32>,
}

impl VuMeter {
    pub fn new(cx: &mut Context, meters: Arc<Meters>, params: Arc<Comp76Params>) -> Handle<'_, Self> {
        let mut handle = Self {
            face: Sprite::new(),
            meters,
            params,
            position: Cell::new(0.0),
            velocity: Cell::new(0.0),
        }
        .build(cx, |_| {});

        // The needle keeps travelling between parameter changes, so it drives
        // its own repaint rather than waiting to be asked.
        let entity = handle.entity();
        let timer = handle.context().add_timer(
            Duration::from_millis(16),
            None,
            move |cx, action| {
                if let TimerAction::Tick(_) = action {
                    cx.needs_redraw();
                    let _ = entity;
                }
            },
        );
        handle.context().start_timer(timer);
        handle
    }

    /// Where the needle is being asked to sit, from 0 at the left end of the
    /// printed scale to 1 at the right.
    ///
    /// Deflection, not decibels: a moving coil's position follows the voltage
    /// through it, so this is the quantity the ballistics below should be
    /// smoothing and the quantity the scale is spaced by.
    fn target(&self) -> f32 {
        match self.params.meter.value() {
            // Gain reduction reads backwards: with the unit idle the needle
            // rests on the 0 mark, and it swings left as the unit works, so
            // 7 dB of reduction puts it on the -7.
            MeterMode::GainReduction => sprites::vu_position(-self.meters.reduction_db()),
            // The reference marks are how far below full scale 0 VU sits.
            MeterMode::Plus4 => sprites::vu_position(self.meters.output_db() + 18.0),
            MeterMode::Plus8 => sprites::vu_position(self.meters.output_db() + 14.0),
            // Switched off, the movement falls back against its stop.
            MeterMode::Off => 0.0,
        }
    }
}

impl View for VuMeter {
    fn element(&self) -> Option<&'static str> {
        Some("comp76-meter")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let b = cx.bounds();
        let scale = cx.scale_factor();
        let lit = self.params.power.value() && self.params.meter.value() != MeterMode::Off;

        // Movement, integrated a frame at a time. Damped enough to settle
        // without hunting, but not so much that it feels dead.
        let target = if lit { self.target() } else { 0.0 };
        let position = self.position.get();
        let velocity = self.velocity.get();
        let acceleration = (target - position) * 0.055 - velocity * 0.30;
        let velocity = velocity + acceleration;
        let position = (position + velocity).clamp(-0.02, 1.02);
        self.position.set(position);
        self.velocity.set(velocity);

        // The movement is a case bolted to the panel, not a picture printed on
        // it, so it casts a shadow like everything else on the faceplate.
        cast_shadow(canvas, b, scale, 7.0, 4.0 * scale);

        // The photographed movement, scale plate and all.
        self.face.draw_rect(canvas, sprites::VU, b.x, b.y, b.w, b.h);

        // The needle turns about the hub and is aimed at the mark it is
        // reading, so it lands on the printed scale wherever it is pointing
        // rather than only at the two ends.
        let pivot_x = b.x + b.w * sprites::VU_HUB.0;
        let pivot_y = b.y + b.h * sprites::VU_HUB.1;
        let (mark_x, mark_y) = sprites::vu_mark(b.x, b.y, b.w, b.h, position);
        let (reach_x, reach_y) = (mark_x - pivot_x, mark_y - pivot_y);
        // A whisker past the mark, the way a needle overhangs its scale.
        let (tip_x, tip_y) = (pivot_x + reach_x * 1.02, pivot_y + reach_y * 1.02);
        // The tail stops short of the hub, which covers it on the real thing.
        let (tail_x, tail_y) = (pivot_x + reach_x * 0.16, pivot_y + reach_y * 0.16);

        canvas.scissor(b.x, b.y, b.w, b.h);
        let mut needle = vg::Path::new();
        needle.move_to(tail_x, tail_y);
        needle.line_to(tip_x, tip_y);
        canvas.stroke_path(
            &needle,
            &vg::Paint::color(rgba(0x000000, 0.20)).with_line_width(3.2 * scale),
        );
        canvas.stroke_path(
            &needle,
            &vg::Paint::color(rgb(0x18_18_1a)).with_line_width(1.7 * scale),
        );
        canvas.reset_scissor();

        // The face goes dark when the meter switch is off.
        if !lit {
            let mut shade = vg::Path::new();
            shade.rect(b.x, b.y, b.w, b.h);
            canvas.fill_path(&shade, &vg::Paint::color(rgba(0x08_0a_08, 0.45)));
        }
    }
}


/// One switch of the meter bank. The four of them select between the
/// positions of a single switch, so pressing one releases the rest by
/// definition rather than by an interlock.
pub struct ModeButton {
    param: ParamWidgetBase,
    cap: Sprite,
    index: usize,
    positions: usize,
}

impl ModeButton {
    pub fn new<L, Params, P, FMap>(
        cx: &mut Context,
        params: L,
        params_to_param: FMap,
        index: usize,
        positions: usize,
    ) -> Handle<'_, Self>
    where
        L: Lens<Target = Params> + Clone,
        Params: 'static,
        P: Param + 'static,
        FMap: Fn(&Params) -> &P + Copy + 'static,
    {
        Self {
            param: ParamWidgetBase::new(cx, params, params_to_param),
            cap: Sprite::new(),
            index,
            positions,
        }
        .build(
            cx,
            ParamWidgetBase::build_view(params, params_to_param, move |cx, data| {
                let value = data.make_lens(|param| param.modulated_normalized_value());
                Binding::new(cx, value, |cx, _| cx.needs_redraw());
            }),
        )
    }

    fn selected(&self) -> bool {
        let steps = (self.positions - 1).max(1) as f32;
        let current = (self.param.modulated_normalized_value() * steps).round() as usize;
        current == self.index
    }
}

impl View for ModeButton {
    fn element(&self) -> Option<&'static str> {
        Some("comp76-mode")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        self.cap
            .draw_button(canvas, cx.bounds(), cx.scale_factor(), self.selected());
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|param_event, _| {
            if let RawParamEvent::ParametersChanged = param_event {
                cx.needs_redraw();
            }
        });
        let index = self.index;
        let steps = (self.positions - 1).max(1) as f32;
        event.map(|window_event, meta| {
            if let WindowEvent::MouseDown(MouseButton::Left) = window_event {
                self.param.begin_set_parameter(cx);
                self.param.set_normalized_value(cx, index as f32 / steps);
                self.param.end_set_parameter(cx);
                cx.needs_redraw();
                meta.consume();
            }
        });
    }
}
