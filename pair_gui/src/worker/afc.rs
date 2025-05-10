// src/worker/afc.rs
use crate::types::{Command, GuiEvent};
use crossbeam::channel::Sender;
use idevice::afc::AfcClient;
use idevice::house_arrest::HouseArrestClient;
use idevice::usbmuxd::{UsbmuxdAddr, UsbmuxdConnection};

/// Handle AFC commands
pub async fn handle_afc(cmd: Command, tx: &Sender<GuiEvent>) {
    if let Command::AfcList { udid, path, container, documents } = cmd {
        // Connect and vend AFC or house_arrest
        let mut mux = UsbmuxdConnection::default().await.unwrap();
        let dev = mux.get_device(&udid).await.unwrap();
        let provider = dev.to_provider(UsbmuxdAddr::default(), "pair-gui-afc");

        let mut client = if let Some(bundle) = container {
            let h = HouseArrestClient::connect(&provider).await.unwrap();
            h.vend_container(&bundle).await.unwrap()
        } else if let Some(bundle) = documents {
            let h = HouseArrestClient::connect(&provider).await.unwrap();
            h.vend_documents(&bundle).await.unwrap()
        } else {
            AfcClient::connect(&provider).await.unwrap()
        };

        // List directory
        match client.list_dir(&path).await {
            Ok(entries) => {
                let _ = tx.send(GuiEvent::AfcListResponse(entries));
            }
            Err(e) => {
                let _ = tx.send(GuiEvent::AfcStatus(format!("List failed: {:?}", e)));
            }
        }
    }
}
