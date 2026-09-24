//! Preset storage.
//!
//! A preset is the panel's parameter values keyed by parameter id. Built-in
//! presets are compiled in and cannot be overwritten; the ones you save go
//! into the user's config directory as one small JSON file each, so they can
//! be copied around and edited by hand.

use nih_plug::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::dsp::Revision;

/// The product, which with the revision's name is the Windows folder.
const PRODUCT: &str = "Comp76Fx";

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
            ("input", 0.0),
            ("output", 18.5),
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
///
/// Per platform, because `$XDG_CONFIG_HOME` and `$HOME` are a Unix
/// convention: neither is normally set on Windows, so the Unix rule found
/// nothing there and saving a preset failed with "no config directory". A
/// host started from a Unix-style shell was worse -- it did find `$HOME`, and
/// wrote presets somewhere no Windows DAW session would look again. Windows
/// keeps per-user application data under `%APPDATA%`, the roaming profile,
/// and a preset is exactly the kind of thing that should roam.
pub fn preset_dir(revision: &Revision) -> Option<PathBuf> {
    // `cfg!` rather than `#[cfg]`, so both rules are compiled everywhere and
    // the Windows one can be tested from the platform it is developed on.
    if cfg!(windows) {
        windows_preset_dir(
            revision,
            std::env::var_os("APPDATA"),
            std::env::var_os("USERPROFILE"),
        )
    } else {
        xdg_preset_dir(
            revision,
            std::env::var_os("XDG_CONFIG_HOME"),
            std::env::var_os("HOME"),
        )
    }
}

/// `%APPDATA%\Comp76Fx Rev A\Presets`, falling back to deriving the roaming
/// directory from `%USERPROFILE%` for the rare host that clears `APPDATA`.
///
/// Takes the variables rather than reading them, so the rule is a pure
/// function and its test does not have to change the environment every other
/// test in the binary is reading.
fn windows_preset_dir(
    revision: &Revision,
    appdata: Option<OsString>,
    userprofile: Option<OsString>,
) -> Option<PathBuf> {
    let base = appdata
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            userprofile
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|home| home.join("AppData").join("Roaming"))
        })?;
    Some(
        base.join(format!("{PRODUCT} {}", revision.name))
            .join("Presets"),
    )
}

/// `$XDG_CONFIG_HOME/comp76fx-rev-a/presets`, or `~/.config` below it. This
/// is also what macOS gets: it is where presets have always been written
/// there, and moving them would lose everyone's.
fn xdg_preset_dir(
    revision: &Revision,
    config_home: Option<OsString>,
    home: Option<OsString>,
) -> Option<PathBuf> {
    let base = config_home
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| home.map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join(revision.slug).join("presets"))
}

/// Every preset, built-in first and then the saved ones in name order.
pub fn load_all(params: &impl Params, revision: &Revision) -> Vec<Preset> {
    let user = preset_dir(revision)
        .map(|dir| load_from(&dir))
        .unwrap_or_default();
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

/// Every saved preset in a folder, with the file it came from, under the
/// name the list shows for it. This is the one place a file is matched to a
/// row, so loading, replacing and deleting all agree on which file is which.
fn read_saved(dir: &Path) -> Vec<(PathBuf, Preset)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            let mut preset: Preset = serde_json::from_str(&text).ok()?;
            preset.built_in = false;
            // A preset whose file was renamed should follow the file.
            if preset.name.trim().is_empty() {
                preset.name = path.file_stem()?.to_str()?.to_string();
            }
            Some((path, preset))
        })
        .collect()
}

fn load_from(dir: &Path) -> Vec<Preset> {
    let mut presets: Vec<Preset> = read_saved(dir)
        .into_iter()
        .map(|(_, preset)| preset)
        .collect();
    presets.sort_by_key(|preset| preset.name.to_lowercase());
    presets
}

/// Whether two preset names are the same name, which is how the list, the
/// replace question and the filesystem all have to see it. Case is not a
/// difference: two presets called `Vocal` and `vocal` cannot be told apart in
/// a list, and on Windows and macOS they would be one file anyway.
fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
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

/// Write a preset out, replacing the saved preset of the same name if there
/// is one.
pub fn save(preset: &Preset, revision: &Revision) -> std::io::Result<PathBuf> {
    let dir = preset_dir(revision).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no config directory to save presets into",
        )
    })?;
    save_into(&dir, preset)
}

fn save_into(dir: &Path, preset: &Preset) -> std::io::Result<PathBuf> {
    if preset.name.trim().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "a preset needs a name",
        ));
    }
    std::fs::create_dir_all(dir)?;
    let path = target(dir, &preset.name);
    let json = serde_json::to_string_pretty(preset)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    std::fs::write(&path, json)?;
    Ok(path)
}

/// The file a preset of this name is written to.
///
/// One of yours with the same name, however it is capitalised, is replaced
/// where it lies. The save dialog has just asked whether to replace it, and
/// building the file name afresh answered yes and replaced nothing: on a case
/// sensitive filesystem `vocal.json` went in beside `Vocal.json`, and there
/// were two.
///
/// Otherwise the name's own file, unless something already holds it. Two
/// different names can clean up to the same file name -- `A/B` and `A_B` --
/// and writing over the other one would lose it without anybody having been
/// asked, so a number goes on the end until the name is free.
fn target(dir: &Path, name: &str) -> PathBuf {
    if let Some((path, _)) = read_saved(dir)
        .into_iter()
        .find(|(_, preset)| same_name(&preset.name, name))
    {
        return path;
    }
    let stem = file_stem(name);
    let mut path = dir.join(format!("{stem}.json"));
    let mut number = 2;
    while path.exists() {
        path = dir.join(format!("{stem} {number}.json"));
        number += 1;
    }
    path
}

/// Remove a saved preset's file.
///
/// The file is located the way `read_saved` identifies it rather than by
/// deriving a name from the preset's own, because a file renamed by hand
/// still shows in the list under the name stored inside it. Deleting the row
/// has to remove the file that row actually came from.
pub fn delete(name: &str, revision: &Revision) -> std::io::Result<()> {
    let dir = preset_dir(revision).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no config directory to delete presets from",
        )
    })?;
    delete_from(&dir, name)
}

fn delete_from(dir: &Path, name: &str) -> std::io::Result<()> {
    match read_saved(dir)
        .into_iter()
        .find(|(_, preset)| same_name(&preset.name, name))
    {
        Some((path, _)) => std::fs::remove_file(path),
        None => Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no saved preset by that name",
        )),
    }
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
    presets
        .iter()
        .filter(|preset| !preset.built_in)
        .any(|preset| same_name(&preset.name, name))
}

/// The MS-DOS device names, which Windows still resolves before it looks at
/// the filesystem. `CON.json` is not a file there: it is the console, and
/// opening it for writing fails whatever directory you are in. The extension
/// does not save you -- the rule is applied to the stem.
const WINDOWS_DEVICE_NAMES: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Turns a preset name into something safe to use as a file name.
///
/// Everything but letters, digits, space, hyphen and underscore becomes an
/// underscore, which covers the characters Windows forbids as well as the
/// ones Unix minds. A stem that is one of Windows' device names gets an
/// underscore appended. Windows' other rule, that a trailing dot or space is
/// stripped silently, needs no code: `trim` has taken the spaces and the dot
/// has already become an underscore.
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
        return "preset".to_string();
    }
    if WINDOWS_DEVICE_NAMES
        .iter()
        .any(|device| stem.eq_ignore_ascii_case(device))
    {
        return format!("{stem}_");
    }
    stem
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::REV_A;

    /// A folder of its own under the system's temporary directory.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("comp76fx-presets-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Self(dir)
        }

        fn files(&self) -> usize {
            std::fs::read_dir(&self.0).map_or(0, |entries| entries.count())
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn preset(name: &str, input: f32) -> Preset {
        Preset {
            name: name.to_string(),
            values: BTreeMap::from([("input".to_string(), input)]),
            built_in: false,
        }
    }

    /// Where a Windows host actually looks. Checked with a path that is
    /// absolute on whatever is running the test, because `is_absolute`
    /// answers by the host's rules and `C:\` is not absolute to Linux.
    #[test]
    fn the_windows_preset_directory_is_under_appdata() {
        let appdata = std::env::temp_dir().join("Roaming");
        let dir = windows_preset_dir(&REV_A, Some(appdata.clone().into_os_string()), None)
            .expect("APPDATA is enough on its own");
        assert_eq!(dir, appdata.join("Comp76Fx Rev A").join("Presets"));
    }

    /// And the fallback, for the rare host that clears `APPDATA`.
    #[test]
    fn a_cleared_appdata_falls_back_to_the_user_profile() {
        let profile = std::env::temp_dir().join("user");
        let from_profile = windows_preset_dir(&REV_A, None, Some(profile.clone().into_os_string()));
        let from_appdata = windows_preset_dir(
            &REV_A,
            Some(profile.join("AppData").join("Roaming").into_os_string()),
            Some(profile.into_os_string()),
        );
        assert!(from_profile.is_some());
        assert_eq!(from_profile, from_appdata);
        assert_eq!(windows_preset_dir(&REV_A, None, None), None);
    }

    /// A relative value is a host bug, and following it would scatter presets
    /// through whatever directory the DAW happened to start in.
    #[test]
    fn a_relative_setting_is_refused_rather_than_followed() {
        assert_eq!(
            windows_preset_dir(&REV_A, Some(OsString::from("AppData")), None),
            None
        );
        assert_eq!(
            xdg_preset_dir(&REV_A, Some(OsString::from(".config")), None),
            None
        );
    }

    /// Unix and macOS keep the rule they have always had, because moving it
    /// would lose everyone's saved presets.
    #[test]
    fn the_unix_preset_directory_is_unchanged() {
        let home = std::env::temp_dir().join("home");
        let expected = Some(home.join(".config").join("comp76fx-rev-a").join("presets"));
        assert_eq!(
            xdg_preset_dir(&REV_A, Some(home.join(".config").into_os_string()), None),
            expected
        );
        assert_eq!(
            xdg_preset_dir(&REV_A, None, Some(home.into_os_string())),
            expected
        );
    }

    #[test]
    fn a_windows_device_name_is_not_used_as_a_file_name() {
        assert_eq!(file_stem("Aux"), "Aux_");
        assert_eq!(file_stem("con"), "con_");
        assert_eq!(file_stem("Auxiliary"), "Auxiliary");
        assert_eq!(file_stem("Clean."), "Clean_");
    }

    /// Replacing has to replace, however the name was capitalised.
    #[test]
    fn replacing_a_preset_writes_over_it_rather_than_beside_it() {
        let scratch = Scratch::new("replace");
        save_into(&scratch.0, &preset("Vocal", 0.1)).unwrap();
        assert!(name_taken("vocal", &load_from(&scratch.0)));
        save_into(&scratch.0, &preset("vocal", 0.2)).unwrap();

        let saved = load_from(&scratch.0);
        assert_eq!(scratch.files(), 1, "the replaced preset is still there");
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].name, "vocal");
        assert_eq!(saved[0].values["input"], 0.2);
    }

    /// Two names that clean up to one file name are still two presets.
    #[test]
    fn names_that_share_a_file_name_do_not_overwrite_each_other() {
        let scratch = Scratch::new("clash");
        save_into(&scratch.0, &preset("A/B", 0.1)).unwrap();
        save_into(&scratch.0, &preset("A_B", 0.2)).unwrap();

        let saved = load_from(&scratch.0);
        let names: Vec<&str> = saved.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names.len(), 2, "one overwrote the other: {names:?}");
        assert!(names.contains(&"A/B") && names.contains(&"A_B"));

        // And deleting one leaves the other.
        delete_from(&scratch.0, "a/b").unwrap();
        let left = load_from(&scratch.0);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].name, "A_B");
    }

    #[test]
    fn a_preset_needs_a_name() {
        let scratch = Scratch::new("unnamed");
        assert!(save_into(&scratch.0, &preset("   ", 0.1)).is_err());
        assert_eq!(scratch.files(), 0);
    }
}
