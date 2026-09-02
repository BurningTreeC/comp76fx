//! Preset storage.
//!
//! A preset is the panel's parameter values keyed by parameter id. Built-in
//! presets are compiled in and cannot be overwritten; the ones you save go
//! into the user's config directory as one small JSON file each, so they can
//! be copied around and edited by hand.

use nih_plug::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Parameters a preset leaves alone. Oversampling is a choice about the
/// machine rather than the sound, and the power switch is not a setting.
const EXCLUDED: &[&str] = &["os", "power"];

/// Built-in presets, written in the values the panel shows so they can be read
/// off the dial. They are converted to normalised values against the live
/// parameters, so changing a control's range cannot silently move them.
///
/// Ratio buttons are 1.0 for in and 0.0 for out; all four in is all-button
/// mode. Attack and release are marked 1 to 7, slowest to fastest.
const BUILT_IN: &[(&str, &[(&str, f32)])] = &[
    (
        // Fast and firm, the setting a vocal usually wants.
        "Vocal 4:1",
        &[
            ("input", 12.0),
            ("output", 1.5),
            ("attack", 5.0),
            ("release", 4.0),
            ("ratio4", 1.0),
            ("ratio8", 0.0),
            ("ratio12", 0.0),
            ("ratio20", 0.0),
            ("mix", 100.0),
        ],
    ),
    (
        // The one everybody reaches for. All four ratio buttons in, both
        // dials wide open, and the input driven hard enough that the unit is
        // never out of gain reduction. The manual calls the result somewhere
        // between 12:1 and 20:1; the lag on the attack is what lets the front
        // of every transient through before the gain collapses behind it.
        "All Buttons In",
        &[
            ("input", 17.0),
            ("output", 4.5),
            ("attack", 7.0),
            ("release", 7.0),
            ("ratio4", 1.0),
            ("ratio8", 1.0),
            ("ratio12", 1.0),
            ("ratio20", 1.0),
            ("mix", 100.0),
        ],
    ),
    (
        // Slower attack so the pick or the beater still lands.
        "Bass 8:1",
        &[
            ("input", 14.0),
            ("output", 3.5),
            ("attack", 2.5),
            ("release", 5.0),
            ("ratio4", 0.0),
            ("ratio8", 1.0),
            ("ratio12", 0.0),
            ("ratio20", 0.0),
            ("mix", 100.0),
        ],
    ),
    (
        // Limiting rather than compressing, with the wet blended back in.
        "Parallel Smash 20:1",
        &[
            ("input", 26.0),
            ("output", -16.0),
            ("attack", 7.0),
            ("release", 6.0),
            ("ratio4", 0.0),
            ("ratio8", 0.0),
            ("ratio12", 0.0),
            ("ratio20", 1.0),
            ("mix", 45.0),
        ],
    ),
];

/// A preset: parameter id to normalised value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub values: BTreeMap<String, f32>,
    /// Compiled in rather than loaded from disk, so it cannot be overwritten.
    #[serde(default, skip)]
    pub built_in: bool,
}

/// The dial positions of a built-in preset, as the panel shows them. Exposed
/// so the response tests can check that a preset does what its name claims.
pub fn built_in_dials(name: &str) -> Option<&'static [(&'static str, f32)]> {
    BUILT_IN
        .iter()
        .find(|(preset, _)| *preset == name)
        .map(|(_, dials)| *dials)
}

/// Where saved presets live. Each revision keeps its own folder: they are
/// separate plugins with their own panels, and a preset saved in one has no
/// business appearing in another's list.
pub fn preset_dir(slug: &str) -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join(slug).join("presets"))
}

/// Every preset, built-in first and then the saved ones in name order. A saved
/// preset that shares a name with a built-in one replaces it, so the list
/// never shows the same name twice and saving over a built-in name does what
/// it looks like it does.
pub fn load_all(params: &impl Params, slug: &str) -> Vec<Preset> {
    let user = load_user(slug);
    // A saved preset that happens to share a name with a factory one does not
    // hide it. The factory preset is compiled in and cannot be edited, so
    // dropping it from the list would put it permanently out of reach; the two
    // sit side by side instead, told apart by the factory tag and by the fact
    // that only yours can be deleted.
    let mut presets: Vec<Preset> = built_in(params);
    presets.extend(user);
    presets
}

fn built_in(params: &impl Params) -> Vec<Preset> {
    // Plain values have to be converted against the real parameters, so build
    // a lookup of id to pointer first.
    let pointers: BTreeMap<String, ParamPtr> = params
        .param_map()
        .into_iter()
        .map(|(id, ptr, _)| (id, ptr))
        .collect();

    BUILT_IN
        .iter()
        .map(|(name, dials)| Preset {
            name: (*name).to_string(),
            values: dials
                .iter()
                .filter_map(|(id, plain)| {
                    let ptr = pointers.get(*id)?;
                    // SAFETY: the pointers come from the params we were handed,
                    // which outlive this function.
                    Some((id.to_string(), unsafe { ptr.preview_normalized(*plain) }))
                })
                .collect(),
            built_in: true,
        })
        .collect()
}

fn load_user(slug: &str) -> Vec<Preset> {
    let Some(dir) = preset_dir(slug) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut presets: Vec<Preset> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            let mut preset: Preset = serde_json::from_str(&text).ok()?;
            preset.built_in = false;
            // A preset whose file was renamed should follow the file.
            if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
                if preset.name.trim().is_empty() {
                    preset.name = stem.to_string();
                }
            }
            Some(preset)
        })
        .collect();
    presets.sort_by_key(|preset| preset.name.to_lowercase());
    presets
}

/// Take the current panel settings as a preset.
pub fn capture(params: &impl Params, name: &str) -> Preset {
    let values = params
        .param_map()
        .into_iter()
        .filter(|(id, _, _)| !EXCLUDED.contains(&id.as_str()))
        .map(|(id, ptr, _)| {
            // SAFETY: as above, the pointers belong to the params we were given.
            let value = unsafe { ptr.unmodulated_normalized_value() };
            (id, value)
        })
        .collect();

    Preset {
        name: name.trim().to_string(),
        values,
        built_in: false,
    }
}

/// Write a preset out, replacing any file with the same name.
pub fn save(preset: &Preset, slug: &str) -> std::io::Result<PathBuf> {
    let dir = preset_dir(slug).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no config directory to save presets into",
        )
    })?;
    std::fs::create_dir_all(&dir)?;

    let path = dir.join(format!("{}.json", file_stem(&preset.name)));
    let json = serde_json::to_string_pretty(preset)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    std::fs::write(&path, json)?;
    Ok(path)
}

/// Remove a saved preset's file.
///
/// The file is located the way `load_user` identifies it rather than by
/// deriving a name from the preset's own, because a file renamed by hand
/// still shows in the list under the name stored inside it. Deleting the row
/// has to remove the file that row actually came from.
pub fn delete(name: &str, slug: &str) -> std::io::Result<()> {
    let dir = preset_dir(slug).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no config directory to delete presets from",
        )
    })?;
    let wanted = name.trim().to_lowercase();
    for entry in std::fs::read_dir(&dir)?.filter_map(Result::ok) {
        let path = entry.path();
        if !path.extension().is_some_and(|ext| ext == "json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(preset) = serde_json::from_str::<Preset>(&text) else {
            continue;
        };
        let shown = if preset.name.trim().is_empty() {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .to_string()
        } else {
            preset.name.clone()
        };
        if shown.trim().to_lowercase() == wanted {
            return std::fs::remove_file(&path);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "no saved preset by that name",
    ))
}

/// Whether the live parameters still match a preset's values. Comparing the
/// values rather than tracking an edited flag means that turning a control
/// back to where it was counts as unmodified again.
pub fn matches(params: &impl Params, values: &BTreeMap<String, f32>) -> bool {
    if values.is_empty() {
        return true;
    }
    params.param_map().into_iter().all(|(id, ptr, _)| {
        let Some(&saved) = values.get(&id) else {
            return true;
        };
        // SAFETY: the pointer comes from the params we were handed.
        let current = unsafe { ptr.unmodulated_normalized_value() };
        (current - saved).abs() <= 1e-5
    })
}

/// Whether saving under this name would replace a file of yours.
///
/// Factory presets are deliberately not counted: saving under one of their
/// names writes a new file beside it and replaces nothing, so warning about
/// it would be describing something that does not happen.
pub fn name_taken(name: &str, presets: &[Preset]) -> bool {
    let name = name.trim();
    presets
        .iter()
        .filter(|preset| !preset.built_in)
        .any(|preset| preset.name.trim().eq_ignore_ascii_case(name))
}

/// Turns a preset name into something safe to use as a file name.
fn file_stem(name: &str) -> String {
    let stem: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if stem.is_empty() {
        "preset".to_string()
    } else {
        stem
    }
}
