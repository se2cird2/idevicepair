use std::path::{Path, PathBuf};
use plist::{Value, Dictionary};
use std::process::Command as SysCmd;

pub fn extract_values(plist: &Value) -> Dictionary {
        if let Value::Dictionary(dict) = plist {
            dict.clone()
        } else {
            Dictionary::new()
        }
    }
pub fn process_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Integer(i) => i.to_string(),
        Value::Boolean(b) => b.to_string(),
        _ => format!("{:?}", value),
    }
}

pub fn reveal_in_file_browser(path: &Path) {
    #[cfg(target_os = "macos")]
    {
        SysCmd::new("open").arg(path).status().ok();
    }
    #[cfg(target_os = "windows")]
    {
        SysCmd::new("explorer").arg(path).status().ok();
    }
    #[cfg(target_os = "linux")]
    {
        SysCmd::new("xdg-open").arg(path).status().ok();
    }
}

pub fn canonical_or_create(dirname: &str) -> PathBuf {
    let path = PathBuf::from(dirname);
    if !path.exists() {
        std::fs::create_dir_all(&path).ok();
    }
    path.canonicalize().unwrap_or(path)
}
