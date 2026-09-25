//! The panel's controls: knobs, the latching push buttons and the meter.

use nih_plug::prelude::Param;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::vizia::vg;
use nih_plug_vizia::widgets::param_base::ParamWidgetBase;
use nih_plug_vizia::widgets::{util::ModifiersExt, RawParamEvent};
use std::cell::Cell;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::sprites::{self, Cap, Placement, Sprite};
use super::style::*;
use crate::meters::{Meters, Reading};
use crate::params::{Comp76Params, MeterMode};

/// Pixels of vertical drag for the full range of a knob.
const DRAG_RANGE: f32 = 240.0;
/// How much finer the drag becomes while shift is held.
const FINE: f32 = 0.15;
/// The panel knobs and the small trim knobs are different castings; a knob
/// smaller than this is a trim knob.
const TRIM_BELOW: f32 = 25.0;

// ---------------------------------------------------------------------------
// Knob
// ---------------------------------------------------------------------------

/// Share of a knob's sweep an OFF position takes below the lowest mark, on a
/// knob that has one. The attack control is engraved OFF and then 1 to 7,
/// eight marks spaced evenly, so OFF takes one step of seven.
pub const OFF_STEP: f32 = 1.0 / 7.0;

pub struct Knob {
    param: ParamWidgetBase,
    /// The switch worked by an OFF position past the fully anticlockwise end
    /// of the dial, on a knob that has one -- the attack control's, which
    /// switches the limiting off.
    off: Option<ParamWidgetBase>,
    radius: f32,
    dragging: bool,
    last_y: f32,
    /// Where along its whole sweep the knob is being moved to, OFF included,
    /// from `0.0` to `1.0`. Kept here because the OFF position is outside the
    /// parameter's own range, so the parameter alone cannot say how far past
    /// the 1 a drag has gone.
    travel: f32,
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
        Self::construct(cx, params, params_to_param, None, radius)
    }

    /// A knob with an OFF position below its lowest mark, which switches
    /// `params_to_off` off. Moving the knob back up switches it on again.
    pub fn with_off_switch<L, Params, P, Q, FMap, GMap>(
        cx: &mut Context,
        params: L,
        params_to_param: FMap,
        params_to_off: GMap,
        radius: f32,
    ) -> Handle<'_, Self>
    where
        L: Lens<Target = Params> + Clone,
        Params: 'static,
        P: Param + 'static,
        Q: Param + 'static,
        FMap: Fn(&Params) -> &P + Copy + 'static,
        GMap: Fn(&Params) -> &Q + Copy + 'static,
    {
        let off = ParamWidgetBase::new(cx, params, params_to_off);
        Self::construct(cx, params, params_to_param, Some(off), radius)
    }

    fn construct<L, Params, P, FMap>(
        cx: &mut Context,
        params: L,
        params_to_param: FMap,
        off: Option<ParamWidgetBase>,
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
            off,
            radius,
            dragging: false,
            last_y: 0.0,
            travel: 0.0,
            face: Sprite::new(if radius >= TRIM_BELOW {
                sprites::KNOB_LARGE
            } else {
                sprites::KNOB_SMALL
            }),
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

    /// Where along its whole sweep the knob sits now, OFF included.
    fn position(&self) -> f32 {
        let value = self.param.modulated_normalized_value().clamp(0.0, 1.0);
        match &self.off {
            None => value,
            Some(off) if off.modulated_normalized_value() < 0.5 => 0.0,
            Some(_) => OFF_STEP + value * (1.0 - OFF_STEP),
        }
    }

    /// Moves the knob to a point along its whole sweep, working the OFF
    /// switch as it crosses half way between OFF and the lowest mark.
    fn move_to(&mut self, cx: &mut EventContext, travel: f32) {
        self.travel = travel.clamp(0.0, 1.0);
        let Some(off) = &self.off else {
            self.param.set_normalized_value(cx, self.travel);
            return;
        };
        let on = self.travel >= OFF_STEP * 0.5;
        if (off.unmodulated_normalized_value() >= 0.5) != on {
            off.set_normalized_value(cx, if on { 1.0 } else { 0.0 });
        }
        if on {
            let value = ((self.travel - OFF_STEP) / (1.0 - OFF_STEP)).clamp(0.0, 1.0);
            self.param.set_normalized_value(cx, value);
        }
    }

    fn begin(&self, cx: &mut EventContext) {
        self.param.begin_set_parameter(cx);
        if let Some(off) = &self.off {
            off.begin_set_parameter(cx);
        }
    }

    fn end(&self, cx: &mut EventContext) {
        self.param.end_set_parameter(cx);
        if let Some(off) = &self.off {
            off.end_set_parameter(cx);
        }
    }

    /// Back to the parameter's default, with any OFF switch on.
    fn restore_default(&mut self, cx: &mut EventContext) {
        self.begin(cx);
        self.param
            .set_normalized_value(cx, self.param.default_normalized_value());
        if let Some(off) = &self.off {
            off.set_normalized_value(cx, 1.0);
        }
        self.end(cx);
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
        self.end(cx);
    }
}

impl View for Knob {
    fn element(&self) -> Option<&'static str> {
        Some("comp76-knob")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let bounds = cx.bounds();
        let r = self.radius * cx.scale_factor();
        // Pick the frame rendered at this angle rather than turning one image,
        // which would carry the lighting round with the knob.
        let position = self.position();
        let frame = (position * (sprites::KNOB_FRAMES - 1) as f32).round() as usize;
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
    /// A button-up can genuinely go missing. The vendored Windows backend
    /// now translates native capture loss into button releases and checks
    /// physical button state on its existing UI frame timer. That repairs both
    /// the native button tracking and vizia's cached state before another drag.
    ///
    /// So the drag is also ended by anything that says the mouse is no longer
    /// down. The `MouseMove` check is an additional fallback, but cannot by
    /// itself repair a missing native release: vizia's cached state would still
    /// say Pressed. Native recovery belongs in the backend, not this widget.
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
                    self.finish(cx);
                    self.restore_default(cx);
                } else {
                    // A press while this knob still believes a drag is running
                    // means the button came up somewhere nothing here ever
                    // heard about, and the widget has been sitting on the
                    // capture ever since.
                    //
                    // Keep this fallback even with native capture recovery:
                    // a fresh down must not nest a second host edit gesture.
                    //
                    // A fresh press is proof on its own: the button cannot go
                    // down without having been up. So the stale drag is closed
                    // here -- releasing the capture and closing the gesture the
                    // host still thinks is open -- and the new one starts
                    // cleanly. That heals the panel on the first click the
                    // player makes when it looks frozen, which is the first
                    // thing anybody tries.
                    self.finish(cx);
                    self.dragging = true;
                    self.last_y = cx.mouse().cursory;
                    self.travel = self.position();
                    cx.capture();
                    cx.focus();
                    cx.set_active(true);
                    self.begin(cx);
                }
                meta.consume();
            }
            WindowEvent::MouseDoubleClick(MouseButton::Left)
            | WindowEvent::MouseDown(MouseButton::Right) => {
                self.finish(cx);
                self.restore_default(cx);
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
                    self.move_to(cx, self.travel + delta);
                    cx.needs_redraw();
                }
            }
            WindowEvent::MouseScroll(_, y) => {
                let step = if cx.modifiers().shift() { 0.005 } else { 0.02 };
                self.begin(cx);
                self.move_to(cx, self.position() + y * step);
                self.end(cx);
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
    cap: Cap,
    /// The other switches in the same bank, which a plain click releases.
    bank: Vec<nih_plug::prelude::ParamPtr>,
}

impl PushButton {
    pub fn new<'a, L, Params, P, FMap>(
        cx: &'a mut Context,
        params: L,
        params_to_param: FMap,
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
            cap: Cap::new(),
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
            .draw(canvas, cx.bounds(), cx.scale_factor(), pressed);
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

                if !latching {
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
    /// Where the needle actually is, and how fast it is travelling, in scale
    /// lengths and scale lengths per second.
    position: Cell<f32>,
    velocity: Cell<f32>,
    /// The last reading, held between audio blocks, and when it arrived.
    reading: Cell<Reading>,
    read_at: Cell<Option<Instant>>,
    /// When the movement was last advanced, and the time since then it has
    /// not yet been advanced through.
    moved_at: Cell<Option<Instant>>,
    unmoved: Cell<f32>,
}

/// The movement, as a spring and a damper: how hard the coil pulls towards
/// the reading, per second squared, and how much of the needle's speed the
/// damping takes, per second. Tuned by eye at a steady 60 frames a second,
/// where they reproduce the ballistics exactly.
const STIFFNESS: f32 = 198.0;
const DAMPING: f32 = 18.0;
/// The movement is advanced in steps of this length whatever the frame rate.
/// Stepping once per drawn frame, which is what it used to do, made the
/// needle faster on a fast display and slower whenever the host held a frame
/// back.
const MOVEMENT_STEP: f32 = 1.0 / 240.0;
/// After a pause this long the movement jumps rather than catching up.
const MOST_UNMOVED: f32 = 0.25;
/// With no audio for this long the host has stopped processing, and a real
/// meter with no signal falls back to rest.
const READING_EXPIRES: f32 = 0.3;

impl VuMeter {
    pub fn new(
        cx: &mut Context,
        meters: Arc<Meters>,
        params: Arc<Comp76Params>,
    ) -> Handle<'_, Self> {
        let mut handle = Self {
            face: Sprite::new(sprites::VU),
            meters,
            params,
            position: Cell::new(0.0),
            velocity: Cell::new(0.0),
            reading: Cell::new(Reading::SILENT),
            read_at: Cell::new(None),
            moved_at: Cell::new(None),
            unmoved: Cell::new(0.0),
        }
        .build(cx, |_| {});

        // The needle keeps travelling between parameter changes, so it drives
        // its own repaint rather than waiting to be asked.
        let timer =
            handle
                .context()
                .add_timer(Duration::from_millis(16), None, move |cx, action| {
                    if let TimerAction::Tick(_) = action {
                        cx.needs_redraw();
                    }
                });
        handle.context().start_timer(timer);
        handle
    }

    /// Where the needle is being asked to sit, from 0 at the left end of the
    /// printed scale to 1 at the right.
    ///
    /// Deflection, not decibels: a moving coil's position follows the voltage
    /// through it, so this is the quantity the ballistics below should be
    /// smoothing and the quantity the scale is spaced by.
    fn target(&self, now: Instant) -> f32 {
        if let Some(reading) = self.meters.take() {
            self.reading.set(reading);
            self.read_at.set(Some(now));
        } else if self
            .read_at
            .get()
            .is_none_or(|at| now.duration_since(at).as_secs_f32() > READING_EXPIRES)
        {
            self.reading.set(Reading::SILENT);
        }
        let reading = self.reading.get();

        match self.params.meter.value() {
            // Gain reduction reads backwards: with the unit idle the needle
            // rests on the 0 mark, and it swings left as the unit works, so
            // 7 dB of reduction puts it on the -7.
            MeterMode::GainReduction => sprites::vu_position(-reading.reduction_db),
            // The reference marks are how far below full scale 0 VU sits.
            MeterMode::Plus4 => sprites::vu_position(reading.output_db + 18.0),
            MeterMode::Plus8 => sprites::vu_position(reading.output_db + 14.0),
            // Switched off, the movement falls back against its stop.
            MeterMode::Off => 0.0,
        }
    }

    /// Advances the movement to `now` towards `target`, in fixed steps.
    fn swing(&self, now: Instant, target: f32) {
        let elapsed = self
            .moved_at
            .get()
            .map_or(0.0, |at| now.duration_since(at).as_secs_f32());
        self.moved_at.set(Some(now));

        let mut pending = (self.unmoved.get() + elapsed).min(MOST_UNMOVED);
        let (mut position, mut velocity) = (self.position.get(), self.velocity.get());
        while pending >= MOVEMENT_STEP {
            let acceleration = (target - position) * STIFFNESS - velocity * DAMPING;
            velocity += acceleration * MOVEMENT_STEP;
            position = (position + velocity * MOVEMENT_STEP).clamp(-0.02, 1.02);
            pending -= MOVEMENT_STEP;
        }
        self.unmoved.set(pending);
        self.position.set(position);
        self.velocity.set(velocity);
    }
}

impl View for VuMeter {
    fn element(&self) -> Option<&'static str> {
        Some("comp76-meter")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let b = cx.bounds();
        let scale = cx.scale_factor();
        let lit = self.params.powered();

        let now = Instant::now();
        // Taken whether or not the meter is lit, so switching it on shows what
        // is happening now rather than what built up while it was off.
        let target = self.target(now);
        self.swing(now, if lit { target } else { 0.0 });
        let position = self.position.get();

        // The movement is a case bolted to the panel, not a picture printed on
        // it, so it casts a shadow like everything else on the faceplate.
        cast_shadow(canvas, b, scale, 7.0, 4.0 * scale);

        // The photographed movement, scale plate and all.
        self.face.draw_rect(canvas, b.x, b.y, b.w, b.h);

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
    cap: Cap,
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
            cap: Cap::new(),
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
            .draw(canvas, cx.bounds(), cx.scale_factor(), self.selected());
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
