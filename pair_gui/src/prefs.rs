use crate::types::Prefs;
use directories::ProjectDirs;
use std::fs;
use std::path::PathBuf;

pub fn load_prefs() -> Prefs {
    if let Some(proj_dirs) = ProjectDirs::from("", "", "pair_gui") {
        let path = proj_dirs.config_dir().join("prefs.json");
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(p) = serde_json::from_str(&data) {
                return p;
            }
        }
    }
    Prefs::default()
}

pub fn save_prefs(p: &Prefs) {
    if let Some(proj_dirs) = ProjectDirs::from("", "", "pair_gui") {
        let dir = proj_dirs.config_dir();
        fs::create_dir_all(dir).ok();
        let path = dir.join("prefs.json");
        if let Ok(data) = serde_json::to_string_pretty(p) {
            fs::write(path, data).ok();
        }
    }
}
