//! 本地配置与预设的读写。

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::model::{PresetFile, Settings};

pub struct ConfigStore {
    root: PathBuf,
}

impl ConfigStore {
    pub fn new(root: PathBuf) -> Self {
        let _ = fs::create_dir_all(&root);
        Self { root }
    }

    fn settings_path(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    fn presets_path(&self) -> PathBuf {
        self.root.join("presets.json")
    }

    /// 读不到或解析失败时回退到默认值——配置损坏不该让应用起不来
    pub fn load_settings(&self) -> Settings {
        fs::read_to_string(self.settings_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        write_atomic(&self.settings_path(), &serde_json::to_string_pretty(settings)?)
    }

    pub fn load_presets(&self) -> PresetFile {
        fs::read_to_string(self.presets_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save_presets(&self, presets: &PresetFile) -> Result<()> {
        write_atomic(
            &self.presets_path(),
            &serde_json::to_string_pretty(presets)?,
        )
    }
}

fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text)?;
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(&tmp, path)?;
    Ok(())
}
