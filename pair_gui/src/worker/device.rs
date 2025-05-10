// src/worker/device.rs
use idevice::usbmuxd::{Connection as UsbConnection, UsbmuxdAddr, UsbmuxdConnection};
use idevice::lockdown::LockdownClient;
use idevice::IdeviceService;
use idevice::provider::IdeviceProvider;
use plist::Value;
use std::{collections::HashMap, path::Path};
use uuid::Uuid;

use crate::util::{extract_values, process_value, reveal_in_file_browser};

/// Scan connected USB devices and return their UDIDs
pub async fn scan_devices() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut mux = UsbmuxdConnection::default().await?;
    let devices = mux.get_devices().await?;
    Ok(devices
        .into_iter()
        .filter(|d| d.connection_type == UsbConnection::Usb)
        .map(|d| d.udid)
        .collect())
}

/// Retrieve just the device name
pub async fn get_device_name(udid: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut mux = UsbmuxdConnection::default().await?;
    let dev = mux.get_device(udid).await?;
    let provider = dev.to_provider(UsbmuxdAddr::default(), "pair-gui");
    let mut lockdown = LockdownClient::connect(&provider).await?;
    if let Ok(pf) = provider.get_pairing_file().await {
        let _ = lockdown.start_session(&pf).await;
    }
    match lockdown.get_value("DeviceName", None).await {
        Ok(val) => {
            if let Value::String(s) = val {
                Ok(s)
            } else {
                Ok(udid.to_string())
            }
        }
        Err(_) => Ok(udid.to_string()),
    }
}

/// Retrieve just the device model identifier
pub async fn get_device_model(udid: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut mux = UsbmuxdConnection::default().await?;
    let dev = mux.get_device(udid).await?;
    let provider = dev.to_provider(UsbmuxdAddr::default(), "pair-gui");
    let mut lockdown = LockdownClient::connect(&provider).await?;
    if let Ok(pf) = provider.get_pairing_file().await {
        let _ = lockdown.start_session(&pf).await;
    }
    match lockdown.get_value("ProductType", None).await {
        Ok(val) => {
            if let Value::String(s) = val {
                Ok(s)
            } else {
                Ok(String::new())
            }
        }
        Err(_) => Ok(String::new()),
    }
}

/// Pair with a device and save the pairing file
pub async fn pair_one(
    output_dir: &Path,
    udid: &str,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let mut mux = UsbmuxdConnection::default().await?;
    let dev = mux.get_device(udid).await?;
    let provider = dev.to_provider(UsbmuxdAddr::default(), "pair-gui");
    let mut lockdown = LockdownClient::connect(&provider).await?;

    let host_id = Uuid::new_v4().to_string().to_uppercase();
    let buid = mux.get_buid().await?;
    let mut pf = lockdown.pair(host_id, buid).await?;
    let _ = lockdown.start_session(&pf).await?;

    pf.udid = Some(dev.udid.clone());
    let data = pf.serialize()?;
    let out_path = output_dir.join(format!("{}.mobiledevicepairing", udid));
    std::fs::write(&out_path, data)?;
    Ok(output_dir.to_path_buf())
}

/// Retrieve all device info as a flat map
pub async fn get_device_info(
    udid: &str,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut mux = UsbmuxdConnection::default().await?;
    let dev = mux.get_device(udid).await?;
    let provider = dev.to_provider(UsbmuxdAddr::default(), "pair-gui");
    let mut lockdown = LockdownClient::connect(&provider).await?;
    if let Ok(pf) = provider.get_pairing_file().await {
        let _ = lockdown.start_session(&pf).await;
    }
    let dict = lockdown.get_all_values().await?;
    let mut info = HashMap::new();
    extract_values("", &Value::Dictionary(dict.clone()), &mut info);
    if let Ok(value) = lockdown.get_value("ProductVersion", None).await {
        info.insert("ProductVersion".to_string(), process_value(&value));
    }
    if let Ok(device_type) = lockdown.idevice.get_type().await {
        info.insert("DeviceType".to_string(), device_type);
    }
    Ok(info)
}
