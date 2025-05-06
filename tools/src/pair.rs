//! pair_gui  GUI front-end for the iOS pairing utility
//! Jackson Coxson  2025

use crossbeam::channel::{unbounded, Receiver, Sender};
use eframe::{egui, App, NativeOptions};
use env_logger;
use idevice::{
    lockdown::LockdownClient,
    usbmuxd::{UsbmuxdAddr, UsbmuxdConnection},
    IdeviceService,
};
use log::info;
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::Command as SysCmd,
    thread,
    time::{Duration, Instant},
};
use tokio::runtime::Runtime;
use uuid::Uuid;

/*  prefs */

#[derive(Serialize, Deserialize, Default)]
struct Prefs {
    output_dir: Option<PathBuf>,
}

fn pref_path() -> PathBuf {
    directories::ProjectDirs::from("com", "stik", "pair_gui")
        .expect("Project dirs")
        .config_dir()
        .join("prefs.json")
}

fn load_prefs() -> Prefs {
    fs::read_to_string(pref_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_prefs(p: &Prefs) {
    let path = pref_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(s) = serde_json::to_string_pretty(p) {
        let _ = fs::write(path, s);
    }
}

/*  messages */

#[derive(Debug)]
enum Command {
    Refresh,
    Pair { udid: String, out_dir: PathBuf },
}

#[derive(Debug)]
enum GuiEvent {
    Devices(Vec<String>),
    Status(String),
}

/*  GUI app */

struct PairApp {
    tx: Sender<Command>,
    rx: Receiver<GuiEvent>,
    devices: Vec<String>,
    selected: Option<String>,
    status: String,
    output_dir: PathBuf,
    last_tick: Instant,
    first_frame: bool,
}

impl PairApp {
    fn new(tx: Sender<Command>, rx: Receiver<GuiEvent>, default_dir: PathBuf) -> Self {
        let prefs = load_prefs();
        Self {
            tx,
            rx,
            devices: Vec::new(),
            selected: None,
            status: "Scanning".into(),
            output_dir: prefs.output_dir.unwrap_or(default_dir),
            last_tick: Instant::now() - Duration::from_secs(10),
            first_frame: true,
        }
    }
}

impl App for PairApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Auto-refresh every 3 seconds
        if self.first_frame || self.last_tick.elapsed() > Duration::from_secs(3) {
            let _ = self.tx.send(Command::Refresh);
            self.last_tick = Instant::now();
            self.first_frame = false;
        }

        // Handle incoming events
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                GuiEvent::Devices(list) => {
                    self.devices = list;
                    if self.devices.len() == 1 {
                        self.selected = Some(self.devices[0].clone());
                    }
                    self.status = format!("{} device(s) connected", self.devices.len());
                }
                GuiEvent::Status(s) => self.status = s,
            }
        }

        // Build UI
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("iOS Pair Utility");

            ui.horizontal(|ui| {
                if ui.button(" Refresh").clicked() {
                    let _ = self.tx.send(Command::Refresh);
                }

                if ui.button(" Browse").clicked() {
                    if let Some(dir) =
                        FileDialog::new().set_directory(&self.output_dir).pick_folder()
                    {
                        self.output_dir = dir;
                        save_prefs(&Prefs {
                            output_dir: Some(self.output_dir.clone()),
                        });
                        self.status =
                            format!("Output dir set to {}", self.output_dir.display());
                    }
                }

                if ui
                    .add_enabled(self.selected.is_some(), egui::Button::new(" Pair"))
                    .clicked()
                {
                    if let Some(udid) = &self.selected {
                        let _ = self.tx.send(Command::Pair {
                            udid: udid.clone(),
                            out_dir: self.output_dir.clone(),
                        });
                        self.status = format!("Pairing {}", udid);
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.label("Save to:");
                ui.monospace(self.output_dir.display().to_string());
            });

            ui.separator();
            ui.label("Connected USB devices:");
            for dev in &self.devices {
                ui.selectable_value(&mut self.selected, Some(dev.clone()), dev);
            }
            ui.separator();
            ui.label(&self.status);
        });
    }
}

/* worker loop */

async fn worker_loop(rx: Receiver<Command>, tx: Sender<GuiEvent>) {
    loop {
        match rx.recv() {
            Ok(Command::Refresh) => match scan_devices().await {
                Ok(list) => {
                    let _ = tx.send(GuiEvent::Devices(list));
                }
                Err(e) => {
                    let _ = tx.send(GuiEvent::Status(format!("Scan error: {e:?}")));
                }
            },

            Ok(Command::Pair { udid, out_dir }) => {
                let _ = tx.send(GuiEvent::Status(format!("Pairing {udid}")));
                match pair_one(&out_dir, &udid).await {
                    Ok(path) => {
                        let _ = tx.send(GuiEvent::Status(format!(" Paired {udid}.")));
                        reveal_in_file_browser(&path);
                    }
                    Err(e) => {
                        let _ = tx.send(GuiEvent::Status(format!(" {udid}: {e:?}")));
                    }
                }
            }

            Err(_) => break, // channel closed
        }
    }
}

/* device scan & pairing */

async fn scan_devices() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut mux = UsbmuxdConnection::default().await?;
    let devices = mux.get_devices().await?;
    Ok(devices
        .into_iter()
        .filter(|d| d.connection_type == idevice::usbmuxd::Connection::Usb)
        .map(|d| d.udid)
        .collect())
}

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
    let out_path = output_dir.join(format!("{udid}.mobiledevicepairing"));
    File::create(&out_path)?.write_all(&data)?;
    info!("Wrote {}", out_path.display());
    Ok(out_path)
}

/* util */

fn canonical_or_create<P: AsRef<Path>>(p: P) -> PathBuf {
    match fs::canonicalize(&p) {
        Ok(abs) => abs,
        Err(_) => {
            let abs = std::env::current_dir().unwrap().join(&p);
            let _ = fs::create_dir_all(&abs);
            abs
        }
    }
}

/// Reveal the new pairing file in the OS file browser (pre-selected if supported).
fn reveal_in_file_browser(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = SysCmd::new("explorer")
            .args(["/select,", path.to_string_lossy().as_ref()])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = SysCmd::new("open")
            .args(["-R", path.to_string_lossy().as_ref()])
            .spawn();
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(dir) = path.parent() {
            let _ = SysCmd::new("xdg-open").arg(dir).spawn();
        }
    }
}

/* entry point */

fn main() -> eframe::Result<()> {
    env_logger::init();

    let (tx_cmd, rx_cmd) = unbounded::<Command>();
    let (tx_evt, rx_evt) = unbounded::<GuiEvent>();

    // Spawn the background worker
    thread::spawn(move || {
        let rt = Runtime::new().expect("Tokio runtime");
        rt.block_on(worker_loop(rx_cmd, tx_evt));
    });

    // Default output directory
    let default_dir = canonical_or_create("pairings");
    let app = PairApp::new(tx_cmd, rx_evt, default_dir);

    // Run the GUI, returning a Result<Box<dyn App>, Box<dyn Error + Send + Sync>>
    eframe::run_native(
        "iOS Pair Utility",
        NativeOptions::default(),
        Box::new(|_| Ok(Box::new(app))),
    )?;

    Ok(())
}

