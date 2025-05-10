use crate::types::{Command, GuiEvent};
use crossbeam::channel::{Receiver, Sender};
use eframe::{App, egui::{self, SidePanel, TopBottomPanel, CentralPanel, ScrollArea}};
use std::path::PathBuf;

/// Main GUI application for pairing iOS devices
pub struct PairApp {
    tx_cmd: Sender<Command>,
    rx_evt: Receiver<GuiEvent>,
    devices: Vec<(String, String)>,
    status: String,
    prefs_dir: PathBuf,
}

impl PairApp {
    pub fn new(tx_cmd: Sender<Command>, rx_evt: Receiver<GuiEvent>, prefs_dir: PathBuf) -> Self {
        Self { tx_cmd, rx_evt, devices: Vec::new(), status: String::new(), prefs_dir }
    }
}

impl App for PairApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle events from the worker
        while let Ok(evt) = self.rx_evt.try_recv() {
            match evt {
                GuiEvent::Devices(devs) => self.devices = devs,
                GuiEvent::Status(s)    => self.status = s,
                _                      => {}
            }
        }

        // Top bar with a Refresh button
        TopBottomPanel::top("top_panel").show(ctx, |ui| {
            if ui.button("Refresh").clicked() {
                let _ = self.tx_cmd.send(Command::Refresh);
            }
        });

        // Left panel listing devices
        SidePanel::left("device_list").show(ctx, |ui| {
            ui.heading("Devices");
            ui.separator();
            ScrollArea::vertical().show(ui, |ui| {
                for (name, udid) in &self.devices {
                    if ui.button(name).clicked() {
                        let _ = self.tx_cmd.send(Command::Pair { udid: udid.clone(), out_dir: self.prefs_dir.clone() });
                    }
                }
            });
        });

        // Central panel showing status messages
        CentralPanel::default().show(ctx, |ui| {
            ui.label(&self.status);
        });
    }
}
