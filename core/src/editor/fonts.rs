//! The panel's typeface, embedded so it never depends on what is installed.
//!
//! Noto Sans, which NIH-plug's Vizia adapter used to embed and the panel has
//! always been lettered in. Vizia 0.4 brings no fonts of its own here, so the
//! faces travel with the plugin: see `assets/fonts/NOTICE`.

use vizia_plug::vizia::prelude::Context;

/// The family name inside both files.
pub const NOTO_SANS: &str = "Noto Sans";

pub fn register_noto_sans(cx: &mut Context) {
    cx.load_font_mem(include_bytes!("../../../assets/fonts/NotoSans-Regular.ttf"));
    cx.load_font_mem(include_bytes!("../../../assets/fonts/NotoSans-Bold.ttf"));
}
