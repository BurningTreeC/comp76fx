//! Small Skia drawing helpers for the panel's shared paths and lighting.
//!
//! The panel was drawn with femtovg before the move to Vizia 0.4, and its
//! drawing is written in femtovg's terms: a path built up and then filled or
//! stroked with a paint. These keep those terms over Skia, so the faceplate,
//! the meter and the buttons read the same as they did and draw the same.

use vizia_plug::vizia::vg as sk;
pub type Color = sk::Color4f;
pub use sk::paint::Cap as LineCap;

pub struct Path(sk::PathBuilder);

impl Default for Path {
    fn default() -> Self {
        Self::new()
    }
}

impl Path {
    pub fn new() -> Self {
        Self(sk::PathBuilder::new())
    }
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.0.add_rect(sk::Rect::from_xywh(x, y, w, h), None, None);
    }
    pub fn rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32) {
        self.0.add_rrect(
            sk::RRect::new_rect_xy(sk::Rect::from_xywh(x, y, w, h), r, r),
            None,
            None,
        );
    }
    /// An ellipse about its centre, as femtovg's is.
    pub fn ellipse(&mut self, x: f32, y: f32, rx: f32, ry: f32) {
        self.0.add_oval(
            sk::Rect::from_xywh(x - rx, y - ry, 2.0 * rx, 2.0 * ry),
            None,
            None,
        );
    }
    pub fn circle(&mut self, x: f32, y: f32, r: f32) {
        self.ellipse(x, y, r, r);
    }
    pub fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((x, y));
    }
    pub fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((x, y));
    }
}

#[derive(Clone)]
pub struct Paint(sk::Paint);

impl Paint {
    pub fn color(color: Color) -> Self {
        let mut paint = sk::Paint::new(color, None);
        paint.set_anti_alias(true);
        Self(paint)
    }
    pub fn with_line_width(mut self, width: f32) -> Self {
        self.0.set_stroke_width(width);
        self
    }
    pub fn with_line_cap(mut self, cap: LineCap) -> Self {
        self.0.set_stroke_cap(cap);
        self
    }
    pub fn linear_gradient(x0: f32, y0: f32, x1: f32, y1: f32, c0: Color, c1: Color) -> Self {
        let mut paint = Self::color(Color::new(1.0, 1.0, 1.0, 1.0));
        let colors = [c0, c1];
        paint.0.set_shader(sk::gradient::shaders::linear_gradient(
            ((x0, y0), (x1, y1)),
            &sk::gradient::Gradient::new(
                sk::gradient::Colors::new_evenly_spaced(&colors, sk::TileMode::Clamp, None),
                sk::gradient::Interpolation::default(),
            ),
            None,
        ));
        paint
    }
    /// `c0` out to `inner`, blending to `c1` at `outer`, as femtovg's is.
    pub fn radial_gradient(x: f32, y: f32, inner: f32, outer: f32, c0: Color, c1: Color) -> Self {
        let mut paint = Self::color(Color::new(1.0, 1.0, 1.0, 1.0));
        let colors = [c0, c0, c1];
        let stops = [0.0, (inner / outer).clamp(0.0, 1.0), 1.0];
        paint.0.set_shader(sk::gradient::shaders::radial_gradient(
            ((x, y), outer),
            &sk::gradient::Gradient::new(
                sk::gradient::Colors::new(&colors, Some(&stops), sk::TileMode::Clamp, None),
                sk::gradient::Interpolation::default(),
            ),
            None,
        ));
        paint
    }
}

/// A feathered rounded rectangle: `inner` inside it, fading to nothing over
/// `feather` across its edge. What femtovg's box gradient was for, a part's
/// shadow on the panel, done the way Skia does it -- the shape blurred --
/// since Skia has no such gradient. The box gradient ramped linearly across
/// `feather`; a blur whose sigma is `feather / sqrt(2 pi)` falls at the same
/// rate where it crosses the edge.
#[allow(clippy::too_many_arguments)]
pub fn feathered_rect(
    canvas: &sk::Canvas,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    feather: f32,
    inner: Color,
) {
    let mut paint = sk::Paint::new(inner, None);
    paint.set_anti_alias(true);
    paint.set_mask_filter(sk::MaskFilter::blur(
        sk::BlurStyle::Normal,
        (feather / std::f32::consts::TAU.sqrt()).max(0.01),
        false,
    ));
    canvas.draw_rrect(
        sk::RRect::new_rect_xy(sk::Rect::from_xywh(x, y, w, h), r, r),
        &paint,
    );
}

pub trait PanelCanvas {
    fn fill_path(&self, path: &Path, paint: &Paint);
    fn stroke_path(&self, path: &Path, paint: &Paint);
    /// Limits drawing to a rectangle until the matching `restore`.
    fn clip_to(&self, x: f32, y: f32, w: f32, h: f32);
    /// Limits drawing to inside a path until the matching `restore`.
    fn clip_to_path(&self, path: &Path);
}

impl PanelCanvas for sk::Canvas {
    fn fill_path(&self, path: &Path, paint: &Paint) {
        self.draw_path(&path.0.snapshot(), &paint.0);
    }
    fn stroke_path(&self, path: &Path, paint: &Paint) {
        let mut stroke = paint.0.clone();
        stroke.set_style(sk::paint::Style::Stroke);
        self.draw_path(&path.0.snapshot(), &stroke);
    }
    fn clip_to(&self, x: f32, y: f32, w: f32, h: f32) {
        self.save();
        self.clip_rect(sk::Rect::from_xywh(x, y, w, h), None, Some(true));
    }
    fn clip_to_path(&self, path: &Path) {
        self.save();
        self.clip_path(&path.0.snapshot(), None, Some(true));
    }
}
