// src/worker/worker_loop.rs
use crate::types::{Command, GuiEvent};
use crossbeam::channel::{Receiver, Sender};
use crate::worker::device::{
    get_device_info, get_device_model, get_device_name, pair_one, scan_devices,
};
use crate::worker::afc::handle_afc;
use crate::util::reveal_in_file_browser;

pub async fn worker_loop(rx: Receiver<Command>, tx: Sender<GuiEvent>) {
    loop {
        match rx.recv() {
            Ok(cmd) => match cmd {
                Command::Refresh => {
                    match scan_devices().await {
                        Ok(udids) => {
                            let mut devices = Vec::new();
                            for udid in udids {
                                let name = get_device_name(&udid).await.unwrap_or_else(|_| udid.clone());
                                let model = get_device_model(&udid).await.unwrap_or_else(|_| String::new());
                                let display = if model.is_empty() {
                                    name.clone()
                                } else {
                                    format!("{} ({})", name, model)
                                };
                                devices.push((udid.clone(), display));
                                if let Ok(info) = get_device_info(&udid).await {
                                    let _ = tx.send(GuiEvent::DeviceInfo { udid: udid.clone(), info });
                                }
                            }
                            let _ = tx.send(GuiEvent::Devices(devices));
                        }
                        Err(e) => {
                            let _ = tx.send(GuiEvent::Status(format!("Error scanning: {:?}", e)));
                        }
                    }
                }
                Command::Pair { udid, out_dir } => {
                    let _ = tx.send(GuiEvent::Status(format!("Pairing {}", udid)));
                    match pair_one(&out_dir, &udid).await {
                        Ok(dir) => {
                            let _ = tx.send(GuiEvent::Status(format!("Successfully paired {}", udid)));
                            reveal_in_file_browser(&dir);
                        }
                        Err(e) => {
                            let _ = tx.send(GuiEvent::Status(format!("Error pairing {}: {:?}", udid, e)));
                        }
                    }
                }
                Command::GetDeviceInfo { udid } => {
                    let _ = tx.send(GuiEvent::Status(format!("Getting info for {}", udid)));
                    match get_device_info(&udid).await {
                        Ok(info) => {
                            let _ = tx.send(GuiEvent::DeviceInfo { udid, info });
                        }
                        Err(e) => {
                            let _ = tx.send(GuiEvent::Status(format!("Error getting device info: {:?}", e)));
                        }
                    }
                }
                // AFC commands
                cmd @ Command::AfcList { .. } => {
                    handle_afc(cmd, &tx).await;
                }
                _ => {}
            },
            Err(_) => break,
        }
    }
}
