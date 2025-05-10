use crate::{
    types::{Command, GuiEvent},
    worker::{device::*, afc::list_files},
};
use crossbeam::channel::{Receiver, Sender};

pub async fn worker_loop(rx: Receiver<Command>, tx: Sender<GuiEvent>) {
    loop {
        match rx.recv() {
            Ok(Command::Refresh) => {
                let _ = tx.send(GuiEvent::Status("Refreshing...".into()));
                let udids = match scan_devices().await {
                    Ok(udids) => udids,
                    Err(e) => {
                        let _ = tx.send(GuiEvent::Status(format!("Error: {e}")));
                        continue;
                    }
                };
                for udid in &udids {
                    if let Ok(info) = get_device_info(udid).await {
                        let _ = tx.send(GuiEvent::DeviceInfo {
                            udid: udid.clone(),
                            info,
                        });
                    }
                }
                let list = udids
                    .iter()
                    .map(|udid| (udid.clone(), udid.clone()))
                    .collect();
                let _ = tx.send(GuiEvent::Devices(list));
            }

            Ok(Command::Pair { udid, out_dir }) => {
                let res = pair_one(&out_dir, &udid).await;
                let _ = match res {
                    Ok(_) => tx.send(GuiEvent::Status(format!("Paired {udid}"))),
                    Err(e) => tx.send(GuiEvent::Status(format!("Pair error: {e}"))),
                };
            }

            Ok(Command::GetDeviceInfo { udid }) => {
                let res = get_device_info(&udid).await;
                match res {
                    Ok(info) => {
                        let _ = tx.send(GuiEvent::DeviceInfo { udid, info });
                    }
                    Err(e) => {
                        let _ = tx.send(GuiEvent::Status(format!("Error: {e}")));
                    }
                }
            }

            Ok(Command::AfcList {
                udid,
                path,
                container,
                documents,
            }) => {
                let _ = tx.send(GuiEvent::Status(format!("Listing: {path}")));
                match list_files(&udid, &path, container.as_deref(), documents.as_deref()).await {
                    Ok(list) => {
                        let output = list.join("\n");
                        let _ = tx.send(GuiEvent::Status(format!(
                            "Contents of {path}:\n{output}"
                        )));
                    }
                    Err(e) => {
                        let _ = tx.send(GuiEvent::Status(format!("AFC error: {e}")));
                    }
                }
            }

            Err(_) => break,
        }
    }
}
