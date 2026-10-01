//! The panel size, which has to survive a session.
//!
//! A plain round trip through the two calls the host makes when it saves and
//! reloads a session. The bug this is here to catch was not in the drawing or
//! in the menu -- both worked -- but in the figure never reaching the state
//! that gets written down.

use comp76fx_core::editor::settings::SCALES;
use comp76fx_core::editor::style::{PANEL_W, WINDOW_H};
use comp76fx_core::editor::{default_state, remember_scale, BASE_DPI};
use comp76fx_core::params::Comp76Params;
use nice_plug::params::Params;

fn fresh() -> Comp76Params {
    Comp76Params::new(default_state())
}

/// Setting the size has to change the state the host reads, not just what
/// vizia draws at. `Editor::size` is computed from this, so if it does not
/// move the host sizes the window for the old scale and the panel ends up
/// drawn larger than the window holding it.
#[test]
fn choosing_a_size_reaches_the_state_the_host_reads() {
    let state = default_state();
    assert_eq!(
        state.user_scale_factor(),
        1.0,
        "a fresh panel opens at 100 %"
    );
    remember_scale(&state, 1.5);
    assert_eq!(
        state.user_scale_factor(),
        1.5,
        "the size was chosen but the state never heard about it"
    );
    let (w, h) = state.inner_logical_size();
    let (sw, sh) = state.scaled_logical_size();
    println!("{w}x{h} logical, {sw}x{sh} at 150 %");
    assert!(
        sw > w && sh > h,
        "the window the host is told to make did not grow"
    );
}

/// And it has to come back, at every size the menu offers.
#[test]
fn the_size_survives_a_session() {
    for scale in SCALES {
        let saved = fresh();
        remember_scale(&saved.editor_state, scale);
        let fields = saved.serialize_fields();

        let restored = fresh();
        restored.deserialize_fields(&fields);
        println!(
            "{scale:.2} saved, {:.2} restored",
            restored.editor_state.user_scale_factor()
        );
        assert_eq!(
            restored.editor_state.user_scale_factor(),
            scale,
            "the panel reopened at a different size than it was left at"
        );
    }
}

/// A plugin whose size has never been touched opens at 100 %, which renders
/// at the 1.5 base: the menu's percentage and the drawing scale are separate.
#[test]
fn an_untouched_panel_opens_at_100_percent_with_1_5_base_dpi() {
    let params = fresh();
    let restored = fresh();
    restored.deserialize_fields(&params.serialize_fields());
    assert_eq!(BASE_DPI, 1.5);
    assert_eq!(restored.editor_state.user_scale_factor(), 1.0);
    assert_eq!(restored.editor_state.rendering_scale_factor(), 1.5);
    assert_eq!(restored.editor_state.scaled_logical_size(), (1680, 441));
}

/// Every size in the menu is relative to the base, not to the panel's own
/// pixels: 200 % renders at 3.0.
#[test]
fn menu_sizes_are_relative_to_the_base_dpi() {
    let state = default_state();
    for scale in SCALES {
        remember_scale(&state, scale);
        assert_eq!(state.user_scale_factor(), scale);
        assert_eq!(state.rendering_scale_factor(), scale * BASE_DPI);
        assert_eq!(
            state.scaled_logical_size(),
            (
                (PANEL_W as f64 * scale * BASE_DPI).round() as u32,
                (WINDOW_H as f64 * scale * BASE_DPI).round() as u32
            ),
            "{scale}"
        );
    }
    remember_scale(&state, 2.0);
    assert_eq!(state.scaled_logical_size(), (3360, 882));
}

/// Choosing a size has to *ask the host to resize the window*, which is a
/// different thing from storing the number and was once the half that was
/// missing: a panel drawn at the new size inside a window still at the old one.
///
/// Under nice-plug the Vizia backend makes that request itself, with an
/// explicit native size, and commits the zoom only from the size the window
/// actually arrives at -- so a host that refuses, or that answers late, as
/// X11 does, leaves the panel and the saved size agreeing with the window.
/// These pin the arithmetic of that transaction at the panel's own size. The
/// backend works in drawing scales, the menu's percentage times the base.
mod resize {
    use comp76fx_core::editor::style::{PANEL_W, WINDOW_H};
    use comp76fx_core::editor::BASE_DPI;
    use vizia_plug::vizia::{request_user_scale, resolve_user_scale};

    const PANEL: (u32, u32) = (PANEL_W as u32, WINDOW_H as u32);

    /// From 100 % to 150 % is from 1.5 to 2.25.
    #[test]
    fn the_host_is_asked_for_the_whole_zoomed_window() {
        let accepted = request_user_scale(BASE_DPI, 1.5 * BASE_DPI, PANEL, |size| {
            assert_eq!(
                (size.width, size.height),
                (PANEL_W as f64 * 2.25, WINDOW_H as f64 * 2.25)
            );
            true
        });
        assert_eq!(accepted, Some(2.25));
    }

    #[test]
    fn a_refused_request_changes_nothing() {
        let result = request_user_scale(1.875, 3.0, PANEL, |size| {
            assert_eq!(
                (size.width, size.height),
                (PANEL_W as f64 * 3.0, WINDOW_H as f64 * 3.0)
            );
            false
        });
        assert_eq!(
            result, None,
            "a refused resize must not leave a zoom pending"
        );
    }

    #[test]
    fn repeated_zoom_changes_make_no_redundant_requests() {
        let mut current = BASE_DPI;
        let mut calls = 0;
        for requested in [0.5, 2.0, 0.75, 1.5, 1.0].map(|scale| scale * BASE_DPI) {
            current = request_user_scale(current, requested, PANEL, |_| {
                calls += 1;
                true
            })
            .unwrap();
            assert_eq!(
                request_user_scale(current, requested, PANEL, |_| panic!("duplicate resize")),
                None
            );
        }
        assert_eq!(calls, 5);
    }

    /// The zoom is read back from the window the host actually made: the
    /// one asked for, give or take a pixel of rounding, or the old one when
    /// it kept that.
    #[test]
    fn the_zoom_follows_the_window_the_host_made() {
        let (w, h) = (PANEL_W as f64, WINDOW_H as f64);
        assert_eq!(resolve_user_scale((w * 2.25, h * 2.25), PANEL, 2.25), 2.25);
        assert_eq!(
            resolve_user_scale((w * 1.8 + 0.4, h * 1.8 - 0.3), PANEL, 1.8),
            1.8
        );
        // Refused: the window is still at 100 %.
        assert_eq!(
            resolve_user_scale((w * BASE_DPI, h * BASE_DPI), PANEL, 2.625),
            BASE_DPI
        );
    }

    /// Every size the menu offers survives the round trip through the
    /// window's whole pixels.
    #[test]
    fn every_menu_size_survives_pixel_rounding() {
        for scale in comp76fx_core::editor::settings::SCALES {
            let dpi = scale * BASE_DPI;
            let made = (
                (PANEL_W as f64 * dpi).round(),
                (WINDOW_H as f64 * dpi).round(),
            );
            assert_eq!(resolve_user_scale(made, PANEL, dpi), dpi, "{scale}");
        }
    }
}

/// Reopening the plugin has to come up at the size that was chosen.
///
/// `ViziaEditor::spawn` reads exactly two things off the stored state --
/// `rendering_scale_factor()`, which it hands to the window description, and
/// `Editor::size()`, which is `scaled_logical_size()`. Both must already read
/// back the chosen size, or the panel reopens drawing at one scale inside a
/// window built for another.
#[test]
fn the_panel_reopens_at_the_size_it_was_left_at() {
    for scale in SCALES {
        let saved = fresh();
        remember_scale(&saved.editor_state, scale);
        let fields = saved.serialize_fields();

        let restored = fresh();
        restored.deserialize_fields(&fields);
        let state = &restored.editor_state;

        assert_eq!(
            state.user_scale_factor(),
            scale,
            "the window would be built to draw at a different scale than chosen"
        );
        assert_eq!(state.rendering_scale_factor(), scale * BASE_DPI);
        let (uw, uh) = state.inner_logical_size();
        assert_eq!(
            state.scaled_logical_size(),
            (
                (uw as f64 * scale * BASE_DPI).round() as u32,
                (uh as f64 * scale * BASE_DPI).round() as u32
            ),
            "the window the host is told to make at {scale:.2} does not match \
             the scale the panel will draw at"
        );
    }
}
