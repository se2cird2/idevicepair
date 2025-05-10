use std::collections::HashMap;
use std::path::PathBuf;

/// Commands sent from the GUI to the worker thread.
#[derive(Debug)]
pub enum Command {
    Refresh,
    Pair {
        udid: String,
        out_dir: PathBuf,
    },
    GetDeviceInfo {
        udid: String,
    },
    /// List a directory over AFC (no manual pairing‐file I/O needed).
    AfcList {
        udid: String,
        path: String,
        container: Option<String>,
        documents: Option<String>,
    },
    // (You can add Download/Upload/Mkdir/etc. variants here later.)
}

/// Events sent from the worker back to the GUI.
#[derive(Debug)]
pub enum GuiEvent {
    Devices(Vec<(String, String)>),
    Status(String),
    DeviceInfo {
        udid: String,
        info: HashMap<String, String>,
    },
    AfcListResponse(Vec<String>),
    AfcStatus(String),
}
