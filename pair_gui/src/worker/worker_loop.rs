use crate::types::{Command, GuiEvent};
use crossbeam::channel::{Receiver, Sender};
use crate::worker::device;
use std::path::PathBuf;

pub async fn worker_loop(rx: Receiver<Command>, tx: Sender<GuiEvent>) {
    tx.send(GuiEvent::Status("Starting worker loop".to_owned())).ok();
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Command::Refresh => {
                let devices = device::scan_devices().await;
                let mut list = Vec::new();
                for udid in devices {
                    if let Some(name) = device::get_device_name(&udid).await {
                        list.push((name, udid));
                    }
                }
                tx.send(GuiEvent::Devices(list)).ok();
            }
            Command::Pair { udid, out_dir } => {
                if let Err(e) = device::pair_one(&udid, out_dir).await {
                    tx.send(GuiEvent::Status(format!("Error pairing {}: {}", udid, e))).ok();
                } else {
                    tx.send(GuiEvent::Status(format!("Paired {}", udid))).ok();
                }
            }
            Command::GetDeviceInfo { udid } => {
                let info = device::get_device_info(&udid).await;
                tx.send(GuiEvent::DeviceInfo { udid, info }).ok();
            }
        }
    }
}
