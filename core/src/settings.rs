//! Preferences shared by the app and the CLI, kept next to the journal in
//! `~/Library/Application Support/fresh/settings.json`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    /// Findings at or inside these paths are never suggested.
    #[serde(default)]
    pub excluded: Vec<PathBuf>,
}

impl Settings {
    fn location() -> PathBuf {
        crate::home().join("Library/Application Support/fresh/settings.json")
    }

    /// The saved settings, or the defaults when nothing is saved yet.
    pub fn load() -> io::Result<Self> {
        match fs::read(Self::location()) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// Writes to a temporary file first, so a crash never leaves half a settings file.
    pub fn save(&self) -> io::Result<()> {
        let location = Self::location();
        if let Some(dir) = location.parent() {
            fs::create_dir_all(dir)?;
        }
        let temporary = location.with_extension("json.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        fs::rename(temporary, location)
    }

    pub fn exclude(&mut self, path: &Path) {
        if !self.excluded.iter().any(|p| p == path) {
            self.excluded.push(path.to_owned());
        }
    }

    pub fn stop_excluding(&mut self, path: &Path) {
        self.excluded.retain(|p| p != path);
    }
}
