//! Tiny JSON persistence with atomic writes and forgiving reads.

use std::fs;
use std::io;
use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Loads `T` from `path`. A missing or corrupt file yields `T::default()` —
/// losing a preferences file must never stop the app from starting.
pub fn load_or_default<T: DeserializeOwned + Default>(path: &Path) -> T {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|err| {
            tracing::warn!(path = %path.display(), %err, "ignoring corrupt file");
            T::default()
        }),
        Err(err) if err.kind() == io::ErrorKind::NotFound => T::default(),
        Err(err) => {
            tracing::warn!(path = %path.display(), %err, "could not read file");
            T::default()
        }
    }
}

/// Writes `value` as pretty JSON. Writes to a sibling temp file and renames it
/// over the target, so a crash mid-write never leaves a truncated file.
pub fn save<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    struct Prefs {
        name: String,
        count: u32,
    }

    #[test]
    fn missing_file_gives_default() {
        let dir = tempfile::tempdir().unwrap();
        let prefs: Prefs = load_or_default(&dir.path().join("nope.json"));
        assert_eq!(prefs, Prefs::default());
    }

    #[test]
    fn corrupt_file_gives_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        fs::write(&path, b"{ not json").unwrap();
        assert_eq!(load_or_default::<Prefs>(&path), Prefs::default());
    }

    #[test]
    fn save_then_load_round_trips_and_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/deeper/prefs.json");
        let prefs = Prefs {
            name: "fulltalk".into(),
            count: 3,
        };
        save(&path, &prefs).unwrap();
        assert_eq!(load_or_default::<Prefs>(&path), prefs);
        assert!(!path.with_extension("json.tmp").exists());
    }
}
