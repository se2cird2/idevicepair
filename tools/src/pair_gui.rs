//! pair_gui  GUI front-end for the iOS pairing utility
//! Jackson Coxson  2025

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use crossbeam::channel::{unbounded, Receiver, Sender};
use eframe::{egui, App, NativeOptions};
use egui::ScrollArea;
use env_logger;
use idevice::{
    lockdown::LockdownClient,
    usbmuxd::{UsbmuxdAddr, UsbmuxdConnection},
    IdeviceService,
};
use idevice::provider::IdeviceProvider;
use plist::{self, Value};
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command as SysCmd,
    thread,
    time::{Duration, Instant},
};
use tokio::runtime::Runtime;
use uuid::Uuid;
use directories::BaseDirs;

/// Commands sent from GUI to worker
#[derive(Debug)]
enum Command {
    Refresh,
    Pair { udid: String, out_dir: PathBuf },
    GetDeviceInfo { udid: String },
}

/// Events sent from worker to GUI
#[derive(Debug)]
enum GuiEvent {
    Devices(Vec<(String, String)>), // (UDID, DisplayName)
    Status(String),
    DeviceInfo { udid: String, info: HashMap<String, String> },
}

/// Persistent preferences
#[derive(Serialize, Deserialize, Default)]
struct Prefs {
    output_dir: Option<PathBuf>,
}

fn load_prefs() -> Prefs {
    if let Some(base) = BaseDirs::new() {
        let mut config_path = base.config_dir().to_path_buf();
        config_path.push("pair_gui_prefs.json");
        if let Ok(data) = fs::read_to_string(&config_path) {
            if let Ok(p) = serde_json::from_str::<Prefs>(&data) {
                return p;
            }
        }
    }
    Prefs::default()
}

/// Save preferences to disk
fn save_prefs(prefs: &Prefs) {
    if let Some(base) = BaseDirs::new() {
        let mut config_dir = base.config_dir().to_path_buf();
        let _ = fs::create_dir_all(&config_dir);
        config_dir.push("pair_gui_prefs.json");
        let _ = fs::write(config_dir, serde_json::to_string_pretty(prefs).unwrap());
    }
}

/// Main application state
struct PairApp {
    tx: Sender<Command>,
    rx: Receiver<GuiEvent>,
    devices: Vec<(String, String)>,
    selected: Option<String>,
    status: String,
    output_dir: PathBuf,
    show_device_info: bool,
    device_info: HashMap<String, HashMap<String, String>>,
    last_tick: Instant,
    first_frame: bool,
}

impl PairApp {
    fn new(tx: Sender<Command>, rx: Receiver<GuiEvent>, default_dir: PathBuf) -> Self {
        Self {
            tx,
            rx,
            devices: Vec::new(),
            selected: None,
            status: String::new(),
            output_dir: default_dir,
            show_device_info: true,
            device_info: HashMap::new(),
            last_tick: Instant::now(),
            first_frame: true,
        }
    }
}

impl App for PairApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.first_frame || self.last_tick.elapsed() > Duration::from_secs(3) {
            let _ = self.tx.send(Command::Refresh);
            self.last_tick = Instant::now();
            self.first_frame = false;
        }

        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                GuiEvent::Devices(list) => {
                    self.devices = list;
                    self.show_device_info = true;
                    if let Some(sel) = &self.selected {
                        if !self.devices.iter().any(|(udid, _)| udid == sel) {
                            self.selected = None;
                        }
                    }
                    if self.selected.is_none() && !self.devices.is_empty() {
                        self.selected = Some(self.devices[0].0.clone());
                    }
                    self.status = format!("{} device(s) connected", self.devices.len());
                }
                GuiEvent::Status(s) => self.status = s,
                GuiEvent::DeviceInfo { udid, info } => {
                    self.device_info.insert(udid.clone(), info);
                    self.status = format!("Device info retrieved for {}", udid);
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.heading("iOS Pair Utility");

                ui.horizontal(|ui| {
                    ui.label(format!("Save directory: {}", self.output_dir.display()));
                });

                ui.horizontal(|ui| {
                    if ui.button("Refresh").clicked() {
                        let _ = self.tx.send(Command::Refresh);
                    }
                    if ui.button("Browse").clicked() {
                        if let Some(dir) = FileDialog::new().set_directory(&self.output_dir).pick_folder() {
                            self.output_dir = dir.clone();
                            save_prefs(&Prefs { output_dir: Some(self.output_dir.clone()) });
                            self.status = format!("Output dir set to {}", self.output_dir.display());
                        }
                    }
                    ui.separator();
                    if ui.add_enabled(self.selected.is_some(), egui::Button::new("Pair")).clicked() {
                        if let Some(udid) = &self.selected {
                            let _ = self.tx.send(Command::Pair { udid: udid.clone(), out_dir: self.output_dir.clone() });
                            self.status = format!("Pairing {}", udid);
                        }
                    }
                });

                ui.separator();
                ui.label("Connected USB devices:");
                for (udid, display) in &self.devices {
                    ui.selectable_value(&mut self.selected, Some(udid.clone()), display);
                }

                if self.show_device_info {
                    if let Some(udid) = &self.selected {
                        if let Some(info) = self.device_info.get(udid) {
                            ui.collapsing("Device Information", |ui| {
                                for key in &[
                                    "ProductName", "ProductVersion", "BuildVersion", "SerialNumber", "DeviceName", "UniqueDeviceID",
                                ] {
                                    if let Some(value) = info.get(*key) {
                                        ui.horizontal(|ui| {
                                            ui.label(format!("{}: ", key));
                                            ui.monospace(value);
                                        });
                                    }
                                }
                                ui.separator();
                                ui.collapsing("All Properties", |ui| {
                                    let mut keys: Vec<&String> = info.keys().collect();
                                    keys.sort();
                                    for key in keys {
                                        if !["ProductName", "ProductVersion", "BuildVersion", "SerialNumber", "DeviceName", "UniqueDeviceID"].contains(&key.as_str()) {
                                            if let Some(value) = info.get(key) {
                                                ui.horizontal(|ui| {
                                                    ui.label(format!("{}: ", key));
                                                    ui.monospace(value);
                                                });
                                            }
                                        }
                                    }
                                });
                            });
                        }
                    }
                }

                ui.separator();
                ui.label(&self.status);
            });
        });
    }
}

/// Background worker loop handling commands
async fn worker_loop(rx: Receiver<Command>, tx: Sender<GuiEvent>) {
    loop {
        match rx.recv() {
            Ok(Command::Refresh) => {
                let udids = match scan_devices().await {
                    Ok(list) => list,
                    Err(e) => { let _ = tx.send(GuiEvent::Status(format!("Error scanning: {e:?}"))); vec![] }
                };
                let mut devices = Vec::new();
                
                for udid in &udids {
                    let name = get_device_name(udid).await.unwrap_or_else(|_| udid.clone());
                    let model = get_device_model(udid).await.unwrap_or_else(|_| "".to_string());
                    let display = if model.is_empty() {
                        name.clone()
                    } else {
                        format!("{} ({})", name, model)
                    };
                    devices.push((udid.clone(), display));
                    
                    // Immediately fetch device info for this device
                    if let Ok(info) = get_device_info(udid).await {
                        let _ = tx.send(GuiEvent::DeviceInfo { udid: udid.clone(), info });
                    }
                }
                
                let _ = tx.send(GuiEvent::Devices(devices.clone()));
            }
            Ok(Command::Pair { udid, out_dir }) => {
                let _ = tx.send(GuiEvent::Status(format!("Pairing {udid}")));
                match pair_one(&out_dir, &udid).await {
                    Ok(dir_path) => {
                        let _ = tx.send(GuiEvent::Status(format!("Successfully paired {udid}")));
                        // Open the directory where the pair file was saved
                        reveal_in_file_browser(&dir_path);
                    },
                    Err(e) => { let _ = tx.send(GuiEvent::Status(format!("Error pairing {udid}: {e:?}"))); }
                }
            }
            Ok(Command::GetDeviceInfo { udid }) => {
                let _ = tx.send(GuiEvent::Status(format!("Getting info for {udid}")));
                match get_device_info(&udid).await {
                    Ok(info) => { let _ = tx.send(GuiEvent::DeviceInfo { udid, info }); }
                    Err(e) => { let _ = tx.send(GuiEvent::Status(format!("Error getting device info: {e:?}"))); }
                }
            }
            Err(_) => break,
        }
    }
}

/// Scan connected USB devices
async fn scan_devices() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut mux = UsbmuxdConnection::default().await?;
    let devices = mux.get_devices().await?;
    Ok(devices.into_iter()
        .filter(|d| d.connection_type == idevice::usbmuxd::Connection::Usb)
        .map(|d| d.udid)
        .collect())
}

/// Retrieve just the device name
async fn get_device_name(udid: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut mux = UsbmuxdConnection::default().await?;
    let dev = mux.get_device(udid).await?;
    let provider = dev.to_provider(UsbmuxdAddr::default(), "pair-gui");
    let mut lockdown = LockdownClient::connect(&provider).await?;
    if let Ok(pf) = provider.get_pairing_file().await {
        let _ = lockdown.start_session(&pf).await;
    }
    match lockdown.get_value("DeviceName", None).await {
        Ok(val) => if let Value::String(s) = val { Ok(s) } else { Ok(udid.to_string()) },
        Err(_) => Ok(udid.to_string()),
    }
}

/// Retrieve just the device model identifier
async fn get_device_model(udid: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut mux = UsbmuxdConnection::default().await?;
    let dev = mux.get_device(udid).await?;
    let provider = dev.to_provider(UsbmuxdAddr::default(), "pair-gui");
    let mut lockdown = LockdownClient::connect(&provider).await?;
    if let Ok(pf) = provider.get_pairing_file().await {
        let _ = lockdown.start_session(&pf).await;
    }
    match lockdown.get_value("ProductType", None).await {
        Ok(val) => if let Value::String(s) = val { Ok(s) } else { Ok(String::new()) },
        Err(_) => Ok(String::new()),
    }
}

/// Pair with a device
async fn pair_one(
    output_dir: &Path,
    udid: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut mux = UsbmuxdConnection::default().await?;
    let dev = mux.get_device(udid).await?;
    let provider = dev.to_provider(UsbmuxdAddr::default(), "pair-gui");
    let mut lockdown = LockdownClient::connect(&provider).await?;

    let host_id = Uuid::new_v4().to_string().to_uppercase();
    let buid = mux.get_buid().await?;
    let mut pf = lockdown.pair(host_id, buid).await?;
    lockdown.start_session(&pf).await?;

    pf.udid = Some(dev.udid.clone());
    let data = pf.serialize()?;
    
    // Use correct filename format: {udid}.mobiledevicepairing
    let out_path = output_dir.join(format!("{}.mobiledevicepairing", udid));
    std::fs::write(&out_path, data)?;
    
    // Return the directory path instead of file path
    Ok(output_dir.to_path_buf())
}

/// Retrieve device info
async fn get_device_info(udid: &str) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut mux = UsbmuxdConnection::default().await?;
    let dev = mux.get_device(udid).await?;
    let provider = dev.to_provider(UsbmuxdAddr::default(), "pair-gui");
    let mut lockdown = LockdownClient::connect(&provider).await?;
    if let Ok(pf) = provider.get_pairing_file().await {
        let _ = lockdown.start_session(&pf).await;
    }
    let dict = lockdown.get_all_values().await?;
    let mut info = HashMap::new();
    extract_values("", &plist::Value::Dictionary(dict.clone()), &mut info);
    if let Ok(value) = lockdown.get_value("ProductVersion", None).await {
        info.insert("ProductVersion".to_string(), process_value(&value));
    }
    if let Ok(device_type) = lockdown.idevice.get_type().await {
        info.insert("DeviceType".to_string(), device_type);
    }
    Ok(info)
}

/// Recursively extract plist values into flat key-value map
fn extract_values(prefix: &str, value: &plist::Value, info: &mut HashMap<String, String>) {
    match value {
        plist::Value::Dictionary(dict) => {
            for (k, v) in dict {
                let new_prefix = if prefix.is_empty() { k.clone() } else { format!("{}.{}", prefix, k) };
                extract_values(&new_prefix, v, info);
                info.insert(new_prefix.clone(), process_value(v));
            }
        }
        plist::Value::Array(arr) => {
            if arr.len() <= 10 {
                for (i, v) in arr.iter().enumerate() {
                    let idx_prefix = format!("{}[{}]", prefix, i);
                    extract_values(&idx_prefix, v, info);
                    info.insert(idx_prefix, process_value(v));
                }
            }
        }
        _ => {}
    }
}

/// Format plist values for display
fn process_value(value: &plist::Value) -> String {
    match value {
        plist::Value::String(s) => s.clone(),
        plist::Value::Integer(i) => i.to_string(),
        plist::Value::Boolean(b) => b.to_string(),
        plist::Value::Data(d) => format!("[{} bytes]", d.len()),
        plist::Value::Date(dt) => format!("{:?}", dt),
        plist::Value::Uid(u) => format!("{:?}", u),
        _ => format!("{:?}", value),
    }
}

/// Reveal file or directory in OS file browser
fn reveal_in_file_browser(path: &Path) {
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

/// Entry point
fn main() -> eframe::Result<()> {
    env_logger::init();
    let prefs = load_prefs();
    let default_dir = prefs.output_dir.clone().unwrap_or_else(|| canonical_or_create("pairings"));
    let (tx_cmd, rx_cmd) = unbounded::<Command>();
    let (tx_evt, rx_evt) = unbounded::<GuiEvent>();
    thread::spawn(move || {
        let rt = Runtime::new().expect("Tokio runtime failed");
        rt.block_on(worker_loop(rx_cmd, tx_evt));
    });
    let app = PairApp::new(tx_cmd, rx_evt, default_dir);
    eframe::run_native(
        "iOS Pair Utility",
        NativeOptions::default(),
        Box::new(|_| Ok(Box::new(app))),
    )?;
    Ok(())
}

/// Load or create preferences directory
fn canonical_or_create(dirname: &str) -> PathBuf {
    let base = BaseDirs::new().unwrap_or_else(|| panic!("Could not determine home directory"));
    let mut dir = base.home_dir().to_path_buf();
    dir.push(dirname);
    if !dir.exists() {
        fs::create_dir_all(&dir).unwrap();
    }
    dir
}