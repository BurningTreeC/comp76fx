//! The real reactive panel, drawn with Skia's raster surface: no display
//! server and no host. What a unit test can say about a panel is limited, but
//! it can say that every view builds, that every pixel of the window is drawn
//! by something, and that the overlays, the save dialog's text entry and the
//! zoom transaction follow the panel's state.
//!
//! Set `COMP76FX_GUI_SNAPSHOTS` to a directory to have each frame written
//! there as a PNG.

use super::*;
use crate::dsp::{REV_A, REV_D, REV_F};
use settings::{Dialog, Menu, UiEvent};
use vizia_plug::vizia::{
    backend::{BackendContext, WindowDescription},
    events::EventManager,
    vg as sk,
};

struct Root;
impl View for Root {}

const SIZE: (u32, u32) = (PANEL_W as u32, WINDOW_H as u32);

/// A window's worth of context with the panel built into it, drawn at
/// `zoom` times its own size.
fn panel(revision: Revision, zoom: f32) -> (BackendContext, Arc<Comp76Params>) {
    let mut cx = Context::new();
    cx.ignore_default_theme = true;
    let mut backend = BackendContext::new(cx);
    let desc = WindowDescription::new().with_inner_size(SIZE.0, SIZE.1);
    backend.add_main_window(Entity::root(), &desc, zoom);
    backend.add_window(Root);
    backend.0.windows.insert(
        Entity::root(),
        WindowState {
            window_description: desc,
            ..Default::default()
        },
    );
    backend.context().add_built_in_styles();
    let params = Arc::new(Comp76Params::new(default_state()));
    build(
        backend.context(),
        params.clone(),
        revision,
        Arc::new(Meters::default()),
    );
    (backend, params)
}

/// Draws a few frames, as the backend would, and checks that nothing was
/// left undrawn. Returns the zoom any view asked the window for.
fn render(backend: &mut BackendContext, name: &str) -> Option<f64> {
    let zoom = backend.context().scale_factor();
    let (w, h) = (
        (SIZE.0 as f32 * zoom).round() as i32,
        (SIZE.1 as f32 * zoom).round() as i32,
    );
    let new_surface = || sk::surfaces::raster_n32_premul((w, h)).unwrap();
    let (mut surface, mut dirty) = (new_surface(), new_surface());
    let mut events = EventManager::new();
    let requested = std::rc::Rc::new(std::cell::Cell::new(None));
    for _ in 0..4 {
        events.flush_events(backend.context(), {
            let requested = requested.clone();
            move |event| {
                if let WindowEvent::SetUserScale(zoom) = event {
                    requested.set(Some(*zoom));
                }
            }
        });
        backend.process_style_updates();
        backend.process_animations();
        backend.process_visual_updates();
        backend.draw(Entity::root(), &mut surface, &mut dirty);
    }

    // The header and the faceplate cover the whole window between them, so a
    // pixel left transparent is one nothing drew. Only whole pixels are
    // checked: at 50 % the panel is 220.5 pixels tall in a window of 221, and
    // the last row is only half covered by anything.
    let image = surface.image_snapshot();
    let info = image.image_info();
    let mut pixels = vec![0u8; (info.width() * info.height() * 4) as usize];
    image.read_pixels(
        info,
        &mut pixels,
        (info.width() * 4) as usize,
        (0, 0),
        sk::image::CachingHint::Allow,
    );
    let covered = (
        (SIZE.0 as f32 * zoom).floor() as usize,
        (SIZE.1 as f32 * zoom).floor() as usize,
    );
    let undrawn = pixels
        .as_chunks::<4>()
        .0
        .chunks(info.width() as usize)
        .take(covered.1)
        .flat_map(|row| &row[..covered.0])
        .filter(|p| p[3] != 255)
        .count();
    assert_eq!(
        undrawn,
        0,
        "{name}: {undrawn} of {}x{} pixels never drawn",
        info.width(),
        info.height()
    );

    if let Ok(directory) = std::env::var("COMP76FX_GUI_SNAPSHOTS") {
        let data = image
            .encode(None, sk::EncodedImageFormat::PNG, None)
            .unwrap();
        std::fs::write(
            std::path::Path::new(&directory).join(format!("{name}.png")),
            data.as_bytes(),
        )
        .unwrap();
    }
    requested.take()
}

fn ui(backend: &mut BackendContext) -> &UiState {
    backend.context().data::<UiState>()
}

#[test]
fn every_revision_draws_its_whole_panel() {
    Runtime::init_on_ui_thread();
    let base = BASE_DPI as f32;
    for (revision, zoom, name) in [
        // 100 %, as each revision opens.
        (REV_A, base, "rev-a"),
        (REV_D, base, "rev-d"),
        (REV_F, base, "rev-f"),
        // The two ends of the size menu, 50 % and 200 %.
        (REV_D, 0.5 * base, "rev-d-50"),
        (REV_D, 2.0 * base, "rev-d-200"),
    ] {
        let (mut backend, _) = panel(revision, zoom);
        render(&mut backend, name);
        // The host moving a control redraws whatever shows it.
        backend
            .context()
            .emit(vizia_plug::widgets::RawParamEvent::ParametersChanged);
        render(&mut backend, &format!("{name}-automated"));
    }
    Runtime::deinit_on_ui_thread();
}

#[test]
fn the_overlays_and_the_save_dialog_follow_the_panel_state() {
    Runtime::init_on_ui_thread();
    let (mut backend, _) = panel(REV_D, BASE_DPI as f32);
    render(&mut backend, "panel");

    backend.context().emit(UiEvent::ToggleSettings);
    render(&mut backend, "settings");
    assert!(ui(&mut backend).open.get_untracked());
    backend.context().emit(UiEvent::ToggleScaleMenu);
    render(&mut backend, "sizes");
    assert_eq!(ui(&mut backend).menu.get_untracked(), Menu::Scale);
    backend.context().emit(UiEvent::Close);
    render(&mut backend, "settings-closed");
    assert!(!ui(&mut backend).open.get_untracked());

    backend.context().emit(UiEvent::TogglePresetMenu);
    render(&mut backend, "presets");
    assert_eq!(ui(&mut backend).menu.get_untracked(), Menu::Preset);
    backend.context().emit(UiEvent::ScrollPresets(1));
    render(&mut backend, "presets-scrolled");
    backend.context().emit(UiEvent::Close);
    render(&mut backend, "presets-closed");

    // The save dialog opens with its name box focused, so typing goes
    // straight into it.
    backend.context().emit(UiEvent::OpenSaveDialog);
    render(&mut backend, "save");
    assert_eq!(ui(&mut backend).dialog.get_untracked(), Dialog::Name);
    assert_eq!(backend.focused_element(), Some("textbox"));
    for c in "Slow Vocal".chars() {
        backend.emit_origin(WindowEvent::CharInput(c));
    }
    render(&mut backend, "typed");
    assert_eq!(ui(&mut backend).name.get_untracked(), "Slow Vocal");
    backend.context().emit(UiEvent::CloseDialog);
    render(&mut backend, "cancelled");
    assert_eq!(ui(&mut backend).dialog.get_untracked(), Dialog::None);
    Runtime::deinit_on_ui_thread();
}

/// Choosing a size asks the window for it and changes nothing else; the
/// panel takes the size on only once the window has it. The window is asked
/// in drawing scale, the menu's percentage times `BASE_DPI`.
#[test]
fn a_size_is_requested_and_taken_on_when_the_window_has_it() {
    Runtime::init_on_ui_thread();
    let (mut backend, params) = panel(REV_D, BASE_DPI as f32);
    render(&mut backend, "before-zoom");

    backend.context().emit(UiEvent::SetScale(1.5));
    let requested = render(&mut backend, "zoom-requested");
    assert_eq!(
        requested,
        Some(2.25),
        "the window was never asked for 150 %"
    );
    assert_eq!(ui(&mut backend).scale.get_untracked(), 1.0);
    assert_eq!(params.editor_state.user_scale_factor(), 1.0);

    // What the backend sends once the host has resized the window.
    backend
        .context()
        .emit(vizia_plug::vizia::UserScaleChanged(2.25));
    render(&mut backend, "zoom-committed");
    assert_eq!(ui(&mut backend).scale.get_untracked(), 1.5);
    assert_eq!(params.editor_state.user_scale_factor(), 1.5);

    // And what it sends when a host refuses the next one: the size the window
    // stayed at.
    backend.context().emit(UiEvent::SetScale(2.0));
    assert_eq!(render(&mut backend, "zoom-refused"), Some(3.0));
    backend
        .context()
        .emit(vizia_plug::vizia::UserScaleChanged(2.25));
    render(&mut backend, "zoom-kept");
    assert_eq!(ui(&mut backend).scale.get_untracked(), 1.5);
    assert_eq!(params.editor_state.user_scale_factor(), 1.5);
    Runtime::deinit_on_ui_thread();
}
