//! pair_gui: GUI front-end for the iOS pairing utility with AFC support
//! Jackson Coxson 2025

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use crossbeam::channel::{unbounded, Receiver, Sender};
use directories::BaseDirs;
use eframe::{egui, App, NativeOptions};
use egui::ScrollArea;
use env_logger;
use idevice::{
    lockdown::LockdownClient,
    usbmuxd::{UsbmuxdAddr, UsbmuxdConnection},
    IdeviceService,
};
use idevice::afc::{AfcClient, opcode::AfcFopenMode};
use idevice::house_arrest::HouseArrestClient;
use idevice::provider::IdeviceProvider;
use plist::Value;
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
use tokio::runtime::Runtime;
use uuid::Uuid;

/// Commands sent from GUI to worker thread
#[derive(Debug)]
enum Command {
    Refresh,
    Pair { udid: String, out_dir: PathBuf },
    GetDeviceInfo { udid: String },
    AfcConnect { udid: String, use_documents: bool },
    AfcListDir { udid: String, path: String },
    AfcDownload { udid: String, remote: String, local: PathBuf },
    AfcUpload { udid: String, local: PathBuf, remote: String },
}

/// Events sent from worker back to GUI
#[derive(Debug)]
enum GuiEvent {
    Devices(Vec<(String, String)>),           // (UDID, DisplayName)
    Status(String),                           // status or error messages
    DeviceInfo { udid: String, info: HashMap<String, String> },
    AfcConnected { udid: String },            // AFC session established
    AfcDirListing { udid: String, path: String, items: Vec<String> },
    AfcError { udid: String, error: String }, // any AFC-related error
    AfcDownloadComplete { udid: String, local: PathBuf },
    AfcUploadComplete { udid: String, remote: String },
}

/// Persistent preferences stored on disk
#[derive(Serialize, Deserialize, Default)]
struct Prefs {
    output_dir: Option<PathBuf>,              // last used save directory
}

/// Load preferences (e.g., output_dir) from config file
fn load_prefs() -> Prefs {
    if let Some(base) = BaseDirs::new() {
        let mut path = base.config_dir().to_path_buf();
        path.push("pair_gui_prefs.json");
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(p) = serde_json::from_str(&data) {
                return p;
            }
        }
    }
    Prefs::default()
}

/// Save preferences to disk
fn save_prefs(prefs: &Prefs) {
    if let Some(base) = BaseDirs::new() {
        let mut dir = base.config_dir().to_path_buf();
        let _ = fs::create_dir_all(&dir);
        dir.push("pair_gui_prefs.json");
        let _ = fs::write(&dir, serde_json::to_string_pretty(prefs).unwrap());
    }
}

/// Main application state, including AFC browsing context
struct PairApp {
    tx: Sender<Command>,                       // to worker
    rx: Receiver<GuiEvent>,                    // from worker
    devices: Vec<(String, String)>,            // connected UDIDs + display names
    selected: Option<String>,                  // currently selected UDID
    status: String,                            // UI status bar text
    output_dir: PathBuf,                       // directory to save files/pairings
    device_info: HashMap<String, HashMap<String, String>>, // cached device info
    afc_connected: HashMap<String, bool>,      // per-device AFC connection flag
    afc_current_dir: HashMap<String, String>,  // current path per device
    afc_listings: HashMap<String, Vec<String>>, // directory entries per device
    last_tick: Instant,                        // for periodic refresh
    first_frame: bool,                         // trigger immediate refresh
}

impl PairApp {
    /// Initialize state
    fn new(tx: Sender<Command>, rx: Receiver<GuiEvent>, default_dir: PathBuf) -> Self {
        PairApp {
            tx,
            rx,
            devices: Vec::new(),
            selected: None,
            status: String::new(),
            output_dir: default_dir,
            device_info: HashMap::new(),
            afc_connected: HashMap::new(),
            afc_current_dir: HashMap::new(),
            afc_listings: HashMap::new(),
            last_tick: Instant::now(),
            first_frame: true,
        }
    }
}

impl App for PairApp {
    /// Called each frame to update UI and process events
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Periodically refresh the device list every 3s, and on first frame
        if self.first_frame || self.last_tick.elapsed() > Duration::from_secs(3) {
            let _ = self.tx.send(Command::Refresh);
            self.last_tick = Instant::now();
            self.first_frame = false;
        }

        // Process incoming events from worker thread
        while let Ok(event) = self.rx.try_recv() {
            match event {
                GuiEvent::Devices(list) => {
                    // Update connected devices list
                    self.devices = list;
                    // Maintain current selection if possible
                    if let Some(sel) = &self.selected {
                        if !self.devices.iter().any(|(id, _)| id == sel) {
                            self.selected = None;
                        }
                    }
                    // Auto-select first device if none selected
                    if self.selected.is_none() && !self.devices.is_empty() {
                        self.selected = Some(self.devices[0].0.clone());
                    }
                    self.status = format!("{} device(s) connected", self.devices.len());
                }
                GuiEvent::Status(msg) => {
                    // Generic status message
                    self.status = msg;
                }
                GuiEvent::DeviceInfo { udid, info } => {
                    // Store detailed device info
                    self.device_info.insert(udid.clone(), info);
                    self.status = format!("Device info loaded: {}", &udid);
                }
                GuiEvent::AfcConnected { udid } => {
                    // AFC session ready
                    self.afc_connected.insert(udid.clone(), true);
                    self.afc_current_dir.insert(udid.clone(), "/".to_string());
                    // Automatically list root dir
                    let _ = self.tx.send(Command::AfcListDir { udid: udid.clone(), path: "/".to_string() });
                    self.status = format!("AFC connected: {}", udid);
                }
                GuiEvent::AfcDirListing { udid, path, items } => {
                    // Update directory listing
                    self.afc_current_dir.insert(udid.clone(), path.clone());
                    self.afc_listings.insert(udid.clone(), items);
                    self.status = format!("Directory: {}", path);
                }
                GuiEvent::AfcError { udid: _, error } => {
                    // Display AFC error
                    self.status = format!("AFC error: {}", error);
                }
                GuiEvent::AfcDownloadComplete { udid: _, local } => {
                    // Notify download complete
                    self.status = format!("Downloaded to {}", local.display());
                }
                GuiEvent::AfcUploadComplete { udid: _, remote } => {
                    // Notify upload complete
                    self.status = format!("Uploaded {}", remote);
                }
            }
        }

        // Build the GUI
        egui::CentralPanel::default().show(ctx, |ui| {
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.heading("iOS Pair Utility with File Browser");

                // Output/pairing directory selection
                ui.horizontal(|ui| {
                    ui.label(format!("Save directory: {}", self.output_dir.display()));
                    if ui.button("Browse").clicked() {
                        if let Some(dir) = FileDialog::new().set_directory(&self.output_dir).pick_folder() {
                            self.output_dir = dir.clone();
                            save_prefs(&Prefs { output_dir: Some(dir) });
                        }
                    }
                });

                // Action buttons: Refresh, Pair, Connect AFC
                ui.horizontal(|ui| {
                    if ui.button("Refresh").clicked() {
                        let _ = self.tx.send(Command::Refresh);
                    }
                    if ui.add_enabled(self.selected.is_some(), egui::Button::new("Pair")).clicked() {
                        if let Some(udid) = &self.selected {
                            let _ = self.tx.send(Command::Pair { udid: udid.clone(), out_dir: self.output_dir.clone() });
                            self.status = format!("Pairing {}...", udid);
                        }
                    }
                    if ui.add_enabled(
                        self.selected.is_some() && !self.afc_connected.get(self.selected.as_ref().unwrap()).copied().unwrap_or(false),
                        egui::Button::new("Connect AFC")
                    ).clicked() {
                        if let Some(udid) = &self.selected {
                            // Connect AFC without house-arrest by default
                            let _ = self.tx.send(Command::AfcConnect { udid: udid.clone(), use_documents: false });
                            self.status = format!("Establishing AFC session for {}...", udid);
                        }
                    }
                });

                ui.separator();
                ui.label("Connected devices:");
                // Device list
                for (udid, name) in &self.devices {
                    ui.selectable_value(&mut self.selected, Some(udid.clone()), name.clone());
                }

                // If a device is selected, show details and AFC browser
                if let Some(udid) = &self.selected {
                    // Collapsible device info
                    if let Some(info) = self.device_info.get(udid) {
                        ui.collapsing("Device Information", |ui| {
                            for (key, value) in info {
                                ui.horizontal(|ui| { ui.label(key); ui.monospace(value); });
                            }
                        });
                    }

                    // AFC file browser
                    if self.afc_connected.get(udid).copied().unwrap_or(false) {
                        ui.collapsing("File Browser", |ui| {
                            let cwd = self.afc_current_dir.get(udid).unwrap();
                            ui.label(format!("Current directory: {}", cwd));
                            if let Some(entries) = self.afc_listings.get(udid) {
                                for entry in entries {
                                    if ui.button(entry).clicked() {
                                        let new_path = format!("{}/{}", cwd.trim_end_matches('/'), entry);
                                        // If trailing slash indicates directory
                                        if entry.ends_with('/') {
                                            let _ = self.tx.send(Command::AfcListDir { udid: udid.clone(), path: new_path });
                                        } else {
                                            // Download file
                                            let local = self.output_dir.join(entry);
                                            let _ = self.tx.send(Command::AfcDownload { udid: udid.clone(), remote: new_path, local });
                                        }
                                    }
                                }
                            }
                            // Upload button
                            if ui.button("Upload File").clicked() {
                                if let Some(file) = FileDialog::new().pick_file() {
                                    let filename = file.file_name().unwrap().to_string_lossy();
                                    let remote = format!("{}/{}", cwd.trim_end_matches('/'), filename);
                                    let _ = self.tx.send(Command::AfcUpload { udid: udid.clone(), local: file.clone(), remote });
                                }
                            }
                        });
                    }
                }

                ui.separator();
                ui.label(format!("Status: {}", self.status));
            });
        });
    }
}

/// Application entry point
fn main() -> eframe::Result<()> {
    env_logger::init();
    // Load or initialize prefs
    let prefs = load_prefs();
    let default_dir = prefs.output_dir.clone().unwrap_or_else(|| {
        // fallback to a 'pairings' folder in home
        let base = BaseDirs::new().expect("Cannot determine home directory");
        let mut d = base.home_dir().to_path_buf();
        d.push("pairings");
        if !d.exists() { let _ = fs::create_dir_all(&d); }
        d
    });

    // Setup channels and Tokio runtime
    let (tx_cmd, rx_cmd) = unbounded::<Command>();
    let (tx_evt, rx_evt) = unbounded::<GuiEvent>();
    let rt = Runtime::new().expect("Failed to start Tokio");

    // Spawn background worker thread
    thread::spawn(move || {
        rt.block_on(worker_loop(rx_cmd, tx_evt));
    });

    // Run the GUI app
    let app = PairApp::new(tx_cmd, rx_evt, default_dir);
    eframe::run_native(
        "iOS Pair Utility",
        NativeOptions::default(),
        Box::new(|_| Box::new(app)),
    )
}

/// Background worker handling all Commands asynchronously
async fn worker_loop(rx: Receiver<Command>, tx: Sender<GuiEvent>) {
    // Map of active AFC clients per device
    let mut afc_clients: HashMap<String, AfcClient> = HashMap::new();

    loop {
        // Wait for next command (blocking)
        let cmd = match rx.recv() {
            Ok(c) => c,
            Err(_) => break,
        };

        match cmd {
            Command::Refresh => {
                // Scan connected devices
                match scan_devices().await {
                    Ok(list) => { let _ = tx.send(GuiEvent::Devices(list)); }
                    Err(e) => { let _ = tx.send(GuiEvent::Status(format!("Scan error: {}", e))); }
                }
            }
            Command::Pair { udid, out_dir } => {
                // Perform pairing and save pairing file
                match pair_device(&udid, &out_dir).await {
                    Ok(path) => { let _ = tx.send(GuiEvent::Status(format!("Paired {} -> {}", udid, path.display()))); }
                    Err(e) => { let _ = tx.send(GuiEvent::Status(format!("Pair error {}: {}", udid, e))); }
                }
            }
            Command::GetDeviceInfo { udid } => {
                // Retrieve full plist of device info
                match fetch_device_info(&udid).await {
                    Ok(info) => { let _ = tx.send(GuiEvent::DeviceInfo { udid, info }); }
                    Err(e) => { let _ = tx.send(GuiEvent::Status(format!("Info error {}: {}", udid, e))); }
                }
            }
            Command::AfcConnect { udid, use_documents } => {
                // Establish AFC client session
                let result: Result<(), String> = async {
                    // Get lockdown provider
                    let mut mux = UsbmuxdConnection::default().await.map_err(|e| e.to_string())?;
                    let dev = mux.get_device(&udid).await.map_err(|e| e.to_string())?;
                    let provider = dev.to_provider(UsbmuxdAddr::default(), "pair-gui");
                    let mut lockdown = LockdownClient::connect(&provider).await.map_err(|e| e.to_string())?;
                    // Start session if pairing file exists
                    if let Ok(pf) = provider.get_pairing_file().await { let _ = lockdown.start_session(&pf).await; }
                    // Choose AFC service
                    let service = if use_documents {
                        // House Arrest to access app documents
                        let ha = HouseArrestClient::connect(&provider).await.map_err(|e| e.to_string())?;
                        ha.vend_documents("com.apple.mobileslideshow").await.map_err(|e| e.to_string())?.take_service().unwrap()
                    } else {
                        // Default misagent-based AFC2 service
                        lockdown.start_service(&Value::String("com.apple.afc2".into())).await.map_err(|e| e.to_string())?
                    };
                    // Connect AFC client
                    let client = AfcClient::connect(&provider).await.map_err(|e| e.to_string())?;
                    afc_clients.insert(udid.clone(), client);
                    Ok(())
                }.await;
                if let Err(err) = result {
                    let _ = tx.send(GuiEvent::AfcError { udid: udid.clone(), error: err });
                } else {
                    let _ = tx.send(GuiEvent::AfcConnected { udid });
                }
            }
            Command::AfcListDir { udid, path } => {
                // List directory entries via AFC
                if let Some(client) = afc_clients.get_mut(&udid) {
                    match client.list_dir(&path).await {
                        Ok(items) => { let _ = tx.send(GuiEvent::AfcDirListing { udid: udid.clone(), path, items }); }
                        Err(e) => { let _ = tx.send(GuiEvent::AfcError { udid: udid.clone(), error: e.to_string() }); }
                    }
                } else {
                    let _ = tx.send(GuiEvent::AfcError { udid: udid.clone(), error: "Not connected".into() });
                }
            }
            Command::AfcDownload { udid, remote, local } => {
                // Download file from device
                if let Some(client) = afc_clients.get_mut(&udid) {
                    match client.open(&remote, AfcFopenMode::RdOnly).await {
                        Ok(mut file) => {
                            match file.read().await {
                                Ok(data) => {
                                    if let Err(e) = tokio::fs::write(&local, data).await { eprintln!("Write error: {}", e); }
                                    let _ = tx.send(GuiEvent::AfcDownloadComplete { udid, local });
                                }
                                Err(e) => { let _ = tx.send(GuiEvent::AfcError { udid: udid.clone(), error: e.to_string() }); }
                            }
                        }
                        Err(e) => { let _ = tx.send(GuiEvent::AfcError { udid: udid.clone(), error: e.to_string() }); }
                    }
                } else {
                    let _ = tx.send(GuiEvent::AfcError { udid: udid.clone(), error: "No AFC client".into() });
                }
            }
            Command::AfcUpload { udid, local, remote } => {
                // Upload file to device
                if let Some(client) = afc_clients.get_mut(&udid) {
                    match tokio::fs::read(&local).await {
                        Ok(data) => {
                            match client.open(&remote, AfcFopenMode::WrOnly).await {
                                Ok(mut file) => {
                                    if let Err(e) = file.write(&data).await { eprintln!("Upload error: {}", e); }
                                    let _ = tx.send(GuiEvent::AfcUploadComplete { udid, remote });
                                }
                                Err(e) => { let _ = tx.send(GuiEvent::AfcError { udid: udid.clone(), error: e.to_string() }); }
                            }
                        }
                        Err(e) => { let _ = tx.send(GuiEvent::AfcError { udid: udid.clone(), error: e.to_string() }); }
                    }
                } else {
                    let _ = tx.send(GuiEvent::AfcError { udid: udid.clone(), error: "No AFC client".into() });
                }
            }
        }
    }
}

// Existing helper functions for scanning, pairing, and info retrieval
async fn scan_devices() -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let mut mux = UsbmuxdConnection::default().await?;
    let devices = mux.get_devices().await?;
    // Convert to (UDID, display) pairs
    Ok(devices.into_iter().map(|d| {
        let disp = format!("{} (ID {})", d.device_name.unwrap_or_default(), d.device_id);
        (d.unique_device_id, disp)
    }).collect())
}

async fn pair_device(udid: &str, out_dir: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut mux = UsbmuxdConnection::default().await?;
    let dev = mux.get_device(udid).await?;
    let provider = dev.to_provider(UsbmuxdAddr::default(), "pair-gui");
    let mut lockdown = LockdownClient::connect(&provider).await?;
    // Pair and save pairing file
    let host_id = Uuid::new_v4().to_string();
    let buid = mux.get_buid().await?;
    let pairing = lockdown.pair(host_id, buid).await?;
    let mut pf_bytes = pairing.serialize()?;
    let out_path = out_dir.join(format!("{}.mobiledevicepairing", udid));
    fs::write(&out_path, &pf_bytes)?;
    Ok(out_path)
}

async fn fetch_device_info(udid: &str) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut mux = UsbmuxdConnection::default().await?;
    let dev = mux.get_device(udid).await?;
    let provider = dev.to_provider(UsbmuxdAddr::default(), "pair-gui");
    let mut lockdown = LockdownClient::connect(&provider).await?;
    if let Ok(pf) = provider.get_pairing_file().await {
        let _ = lockdown.start_session(&pf).await;
    }
    let dict = lockdown.get_all_values().await?;
    let mut map = HashMap::new();
    for (k, v) in dict {
        if let Value::String(s) = v { map.insert(k, s); }
    }
    Ok(map)
}
