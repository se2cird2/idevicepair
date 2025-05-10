// pair_gui/src/worker/common.rs

use std::{fs, path::Path};
use idevice::{
    lockdown::LockdownClient,
    provider::{BoxedProvider, IdeviceProvider},
    usbmuxd::{UsbmuxdAddr, UsbmuxdConnection},
    IdeviceService,
};
use anyhow::Result;

/// Load or perform pairing, then return a ready-to-use provider.
///
/// - `udid`: device identifier (e.g. "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx").
/// - `pairing_file`: optional path to an existing `.mobiledevicepairing`.
/// - `tag`: a human-readable tag for usbmuxd.
pub async fn get_provider(
    udid: &str,
    pairing_file: Option<&Path>,
    tag: &str,
) -> Result<BoxedProvider> {
    // connect to usbmuxd and grab the device handle
    let mut mux = UsbmuxdConnection::default().await?;
    let dev = mux.get_device(udid).await?;
    let provider = dev.to_provider(UsbmuxdAddr::default(), tag);

    // if the user supplied a pairing file, read it and start a lockdown session
    if let Some(pf_path) = pairing_file {
        let raw = fs::read(pf_path)?;
        // NOTE: your crate’s PairingRecord type has a from_bytes or similar:
        let pairing = idevice::pairing_file::PairingRecord::deserialize(&raw)?;
        let mut lockdown = LockdownClient::connect(&provider).await?;
        lockdown.start_session(&pairing).await?;
    }

    Ok(provider)
}
