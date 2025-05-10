use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use std::collections::HashMap;

#[derive(Debug)]
pub enum Command {
    Refresh,
    Pair { udid: String, out_dir: PathBuf },
    GetDeviceInfo { udid: String },
}

#[derive(Debug)]
pub enum GuiEvent {
    Devices(Vec<(String, String)>),
    Status(String),
    DeviceInfo { udid: String, info: HashMap<String, String> },
}

#[derive(Serialize, Deserialize, Default)]
pub struct Prefs {
    pub output_dir: Option<PathBuf>,
}
