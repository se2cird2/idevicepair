use std::{collections::HashMap, path::PathBuf, time::{Duration, Instant}};

use crossbeam::channel::{Receiver, Sender};
use eframe::{egui::{self, ScrollArea}, App};
use rfd::FileDialog;

use crate::{
    prefs::{load_prefs, save_prefs, Prefs},
    types::{Command, GuiEvent},
};

pub struct PairApp {
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
    pub fn new(tx: Sender<Command>, rx: Receiver<GuiEvent>, default_dir: PathBuf) -> Self {
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
                            let _ = self.tx.send(Command::Pair {
                                udid: udid.clone(),
                                out_dir: self.output_dir.clone(),
                            });
                            self.status = format!("Pairing {}", udid);
                        }
                    }
                    if ui.add_enabled(self.selected.is_some(), egui::Button::new("AFC List /Documents")).clicked() {
                        if let Some(udid) = &self.selected {
                            let _ = self.tx.send(Command::AfcList {
                                udid: udid.clone(),
                                bundle_id: "com.apple.DocumentsApp".to_string(),
                                container: false,
                            });
                            self.status = "Listing /Documents...".into();
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
                                    "ProductName", "ProductVersion", "BuildVersion",
                                    "SerialNumber", "DeviceName", "UniqueDeviceID",
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
                                        if !["ProductName", "ProductVersion", "BuildVersion", "SerialNumber", "DeviceName", "UniqueDeviceID"]
                                            .contains(&key.as_str())
                                        {
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
