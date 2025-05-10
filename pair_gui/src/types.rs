// src/types.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Command {
    Refresh,
    Pair { udid: String, out_dir: PathBuf },
    GetDeviceInfo { udid: String },
    AfcList {
        udid: String,
        path: String,
        container: Option<String>,
        documents: Option<String>,
    },
    // (You can add AfcDownload, AfcUpload, AfcMkdir, AfcRemove, AfcInfo here.)
}

#[derive(Debug)]
pub enum GuiEvent {
    Devices(Vec<(String, String)>),
    Status(String),
    DeviceInfo { udid: String, info: HashMap<String, String> },
    AfcListResponse(Vec<String>),
    AfcStatus(String),
}
