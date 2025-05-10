// src/ui/app.rs
use crate::types::{Command, GuiEvent};
use crossbeam::channel::{Receiver, Sender};
use eframe::{App, egui};
use egui::{CentralPanel, ScrollArea, SidePanel, TopBottomPanel};
use rfd::FileDialog;
use std::{collections::HashMap, path::PathBuf};
use std::time::{Duration, Instant};

enum Mode {
    Pairing,
    Files,
}

pub struct PairApp {
    tx_cmd: Sender<Command>,
    rx_evt: Receiver<GuiEvent>,
    devices: Vec<(String, String)>,
    selected: Option<String>,
    status: String,
    output_dir: PathBuf,
    show_device_info: bool,
    device_info: HashMap<String, HashMap<String, String>>,
    last_tick: Instant,
    first_frame: bool,

    mode: Mode,
    afc_path: String,
    afc_entries: Vec<String>,
    selected_file: Option<String>,
    afc_container: Option<String>,
    afc_documents: Option<String>,
}

impl PairApp {
    pub fn new(
        tx_cmd: Sender<Command>,
        rx_evt: Receiver<GuiEvent>,
        default_dir: PathBuf,
    ) -> Self {
        Self {
            tx_cmd,
            rx_evt,
            devices: Vec::new(),
            selected: None,
            status: String::new(),
            output_dir: default_dir,
            show_device_info: true,
            device_info: HashMap::new(),
            last_tick: Instant::now(),
            first_frame: true,

            mode: Mode::Pairing,
            afc_path: "/".to_string(),
            afc_entries: Vec::new(),
            selected_file: None,
            afc_container: None,
            afc_documents: None,
        }
    }
}

impl App for PairApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Periodic refresh
        if self.first_frame || self.last_tick.elapsed() > Duration::from_secs(3) {
            let _ = self.tx_cmd.send(Command::Refresh);
            self.last_tick = Instant::now();
            self.first_frame = false;
        }

        // Handle incoming events
        while let Ok(evt) = self.rx_evt.try_recv() {
            match evt {
                GuiEvent::Devices(list) => {
                    self.devices = list;
                    self.status = format!("{} device(s)", self.devices.len());
                    if self.selected.is_none() && !self.devices.is_empty() {
                        self.selected = Some(self.devices[0].0.clone());
                    }
                }
                GuiEvent::Status(s) => {
                    self.status = s;
                }
                GuiEvent::DeviceInfo { udid, info } => {
                    self.device_info.insert(udid.clone(), info);
                }
                GuiEvent::AfcListResponse(entries) => {
                    self.afc_entries = entries;
                }
                GuiEvent::AfcStatus(msg) => {
                    self.status = msg;
                }
            }
        }

        // Top panel: mode switch
        TopBottomPanel::top("mode_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(matches!(self.mode, Mode::Pairing), "Pairing")
                    .clicked()
                {
                    self.mode = Mode::Pairing;
                }
                if ui
                    .selectable_label(matches!(self.mode, Mode::Files), "Files")
                    .clicked()
                {
                    self.mode = Mode::Files;
                }
            });
        });

        match self.mode {
            Mode::Pairing => {
                SidePanel::left("pairing_list").show(ctx, |ui| {
                    ui.heading("Devices");
                    ScrollArea::vertical().show(ui, |ui| {
                        for (udid, name) in &self.devices {
                            if ui
                                .selectable_label(self.selected.as_ref() == Some(udid), name)
                                .clicked()
                            {
                                self.selected = Some(udid.clone());
                            }
                        }
                    });
                });

                CentralPanel::default().show(ctx, |ui| {
                    ui.vertical(|ui| {
                        ui.label(format!("Selected: {:?}", self.selected));
                        ui.horizontal(|ui| {
                            if ui.button("Refresh").clicked() {
                                let _ = self.tx_cmd.send(Command::Refresh);
                            }
                            if ui.button("Browse").clicked() {
                                if let Some(dir) =
                                    FileDialog::new().set_directory(&self.output_dir).pick_folder()
                                {
                                    self.output_dir = dir.clone();
                                }
                            }
                            if ui
                                .add_enabled(self.selected.is_some(), egui::Button::new("Pair"))
                                .clicked()
                            {
                                if let Some(udid) = &self.selected {
                                    let _ = self.tx_cmd.send(Command::Pair {
                                        udid: udid.clone(),
                                        out_dir: self.output_dir.clone(),
                                    });
                                }
                            }
                        });
                        ui.separator();
                        ui.label(&self.status);
                    });
                });
            }
            Mode::Files => {
                CentralPanel::default().show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Path:");
                        ui.text_edit_singleline(&mut self.afc_path);
                        if ui.button("List").clicked() {
                            if let Some(udid) = &self.selected {
                                let _ = self.tx_cmd.send(Command::AfcList {
                                    udid: udid.clone(),
                                    path: self.afc_path.clone(),
                                    container: self.afc_container.clone(),
                                    documents: self.afc_documents.clone(),
                                });
                            }
                        }
                    });
                    ui.separator();
                    ScrollArea::vertical().show(ui, |ui| {
                        for entry in &self.afc_entries {
                            if ui
                                .selectable_label(
                                    self.selected_file.as_ref() == Some(entry),
                                    entry,
                                )
                                .clicked()
                            {
                                self.selected_file = Some(entry.clone());
                            }
                        }
                    });
                    ui.separator();
                    ui.label(&self.status);
                });
            }
        }

        TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.label(&self.status);
        });
    }
}
