//! Small persisted settings (last folder, window geometry).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub last_dir: Option<String>,
    #[serde(default)]
    pub window_size: Option<[f32; 2]>,
    #[serde(default)]
    pub window_pos: Option<[f32; 2]>,
    /// Font size (points) of the prompt text boxes.
    #[serde(default)]
    pub prompt_font_size: Option<f32>,
}

pub fn data_dir() -> PathBuf {
    // Test / portable override: point the app at another settings folder.
    if let Some(dir) = std::env::var_os("COMFYPNG_DATA_DIR") {
        return PathBuf::from(dir);
    }
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("ComfyPNG")
}

fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read(settings_path())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            #[cfg(debug_assertions)]
            eprintln!("saving settings to {}", path.display());
            if let Err(err) = std::fs::write(&path, text) {
                let _ = &err;
                #[cfg(debug_assertions)]
                eprintln!("settings write failed ({}): {err}", path.display());
            }
        }
    }
}
