// src/util.rs
use plist::Value;
use std::{collections::HashMap, path::Path, path::PathBuf};
use std::process::Command as SysCmd;

/// Recursively extract plist values into a flat key-value map
pub fn extract_values(prefix: &str, value: &Value, info: &mut HashMap<String, String>) {
    match value {
        Value::Dictionary(dict) => {
            for (k, v) in dict {
                let new_prefix = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", prefix, k)
                };
                extract_values(&new_prefix, v, info);
                info.insert(new_prefix.clone(), process_value(v));
            }
        }
        Value::Array(arr) => {
            if arr.len() <= 10 {
                for (i, v) in arr.iter().enumerate() {
                    let idx_prefix = format!("{}[{}]", prefix, i);
                    extract_values(&idx_prefix, v, info);
                    info.insert(idx_prefix.clone(), process_value(v));
                }
            }
        }
        _ => {}
    }
}

/// Format plist values for display
pub fn process_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Integer(i) => i.to_string(),
        Value::Boolean(b) => b.to_string(),
        Value::Data(d) => format!("[{} bytes]", d.len()),
        Value::Date(dt) => format!("{:?}", dt),
        Value::Uid(u) => format!("{:?}", u),
        _ => format!("{:?}", value),
    }
}

/// Reveal file or directory in OS file browser
pub fn reveal_in_file_browser(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = SysCmd::new("explorer").arg(path).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = SysCmd::new("open").arg(path).spawn();
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(dir) = path.parent() {
            let _ = SysCmd::new("xdg-open").arg(dir).spawn();
        }
    }
}

/// Ensure a directory exists, returning its canonical path
pub fn canonical_or_create(dirname: &str) -> PathBuf {
    let path = PathBuf::from(dirname);
    if !path.exists() {
        std::fs::create_dir_all(&path).ok();
    }
    path.canonicalize().unwrap_or(path)
}
