use idevice::{usbmuxd::UsbmuxdConnection, lockdown::LockdownClient};
use plist::Value;
use std::collections::HashMap;
use std::path::PathBuf;

pub async fn scan_devices() -> Vec<String> {
    // TODO: implement device scanning
    Vec::new()
}

pub async fn get_device_name(udid: &str) -> Option<String> {
    // TODO: fetch device name
    None
}

pub async fn get_device_model(udid: &str) -> Option<String> {
    // TODO: fetch device model
    None
}

pub async fn pair_one(udid: &str, out_dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    // TODO: implement pairing logic
    Ok(())
}

pub async fn get_device_info(udid: &str) -> HashMap<String, String> {
    // TODO: fetch device info
    HashMap::new()
}
