//! What the browser keeps about own data: the answer to the first-visit choice, the saved
//! profiles and, when the visitor chose so, the values of their keys. Also the exported file.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::super::storage::DemoStorage;
use super::Secrets;
use super::profile::{Profile, VERSION};

/// The saved state: one JSON document of the demo's storage. It has no `Debug`, so a kept key
/// cannot reach a log line by accident.
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Saved {
    /// True once the first-visit choice has been answered. It is then never offered again.
    pub(crate) asked: bool,
    /// True while own data is the source in use. The demo then connects `current` when it
    /// starts.
    pub(crate) own: bool,
    /// Name of the profile in use, or of the one used last.
    pub(crate) current: Option<String>,
    /// The saved profiles, in the order they were first saved. Names are unique.
    pub(crate) profiles: Vec<Profile>,
    /// The key values of the profiles whose keys the visitor chose to keep, by profile name.
    /// The keys of a profile that is not listed are asked for again at every start.
    pub(crate) keys: Vec<(String, Secrets)>,
}

impl Saved {
    /// The saved state. The default when nothing is saved yet or the document cannot be read.
    pub(crate) fn load(storage: &DemoStorage) -> Self {
        let document = storage.own_data();
        let saved = document.and_then(|document| serde_json::from_str(&document).ok());
        saved.unwrap_or_default()
    }

    /// Writes the state as the own-data document. The demo's storage persists it at the end
    /// of the frame.
    pub(crate) fn save(&self, storage: &DemoStorage) {
        match serde_json::to_string(self) {
            Ok(document) => storage.save_own_data(&document),
            Err(error) => log::warn!("the own-data state was not saved: {error}"),
        }
    }
}

/// The file an export writes: `profile` as indented JSON. A profile names its keys and never
/// holds their values, so the file holds none.
pub(crate) fn export(profile: &Profile) -> String {
    // A profile holds text, numbers and lists only, which always serialise.
    serde_json::to_string_pretty(profile).unwrap_or_default()
}

/// The profile of an exported file. The error says why the file cannot be used: it is not
/// JSON, it does not describe a profile, or it was written by a newer version of the demo.
pub(crate) fn import(text: &str) -> Result<Profile, String> {
    let file: Value = serde_json::from_str(text).map_err(|error| format!("not JSON: {error}"))?;
    // Read first, so that a newer file is named as such whatever else it holds.
    let version = file.get("version").and_then(Value::as_u64);
    if version.is_some_and(|version| version > u64::from(VERSION)) {
        return Err("written by a newer version of the demo".to_owned());
    }
    // Every field of a profile has a default, so any object would read as one.
    if file.get("name").is_none() {
        return Err("not a profile: it has no name".to_owned());
    }
    serde_json::from_value(file).map_err(|error| format!("not a profile: {error}"))
}

/// `name` as the stem of a file name: every character that is not an ASCII letter, a digit,
/// `-` or `_` becomes `_`, and an empty name becomes `unnamed`. A profile name or a symbol
/// that was typed or imported can therefore not carry a path to the place a file is written.
pub(crate) fn file_stem(name: &str) -> String {
    if name.is_empty() {
        return "unnamed".to_owned();
    }
    let plain = |letter: char| letter.is_ascii_alphanumeric() || matches!(letter, '-' | '_');
    let stem = name
        .chars()
        .map(|letter| if plain(letter) { letter } else { '_' });
    stem.collect()
}
