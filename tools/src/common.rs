// Jackson Coxson
// Common functions between tools

use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
};

use idevice::{
    pairing_file::PairingFile,
    provider::{IdeviceProvider, TcpProvider},
    usbmuxd::{UsbmuxdAddr, UsbmuxdConnection},
};

pub async fn get_provider(
    udid: Option<&String>,
    host: Option<&String>,
    pairing_file: Option<&String>,
    label: &str,
) -> Result<Box<dyn IdeviceProvider>, String> {
    let provider: Box<dyn IdeviceProvider> = if udid.is_some() {
        let udid = udid.unwrap();

        let mut usbmuxd = if let Ok(var) = std::env::var("USBMUXD_SOCKET_ADDRESS") {
            let socket = SocketAddr::from_str(&var).expect("Bad USBMUXD_SOCKET_ADDRESS");
            let socket = tokio::net::TcpStream::connect(socket)
                .await
                .expect("unable to connect to socket address");
            UsbmuxdConnection::new(Box::new(socket), 1)
        } else {
            UsbmuxdConnection::default()
                .await
                .expect("Unable to connect to usbmxud")
        };

        let dev = match usbmuxd.get_device(udid).await {
            Ok(d) => d,
            Err(e) => {
                return Err(format!("Device not found: {e:?}"));
            }
        };
        Box::new(dev.to_provider(UsbmuxdAddr::from_env_var().unwrap(), label))
    } else if host.is_some() && pairing_file.is_some() {
        let host = match IpAddr::from_str(host.unwrap()) {
            Ok(h) => h,
            Err(e) => {
                return Err(format!("Invalid host: {e:?}"));
            }
        };
        let pairing_file = match PairingFile::read_from_file(pairing_file.unwrap()) {
            Ok(p) => p,
            Err(e) => {
                return Err(format!("Unable to read pairing file: {e:?}"));
            }
        };

        Box::new(TcpProvider {
            addr: host,
            pairing_file,
            label: "ideviceinfo-jkcoxson".to_string(),
        })
    } else {
        let mut usbmuxd = if let Ok(var) = std::env::var("USBMUXD_SOCKET_ADDRESS") {
            let socket = SocketAddr::from_str(&var).expect("Bad USBMUXD_SOCKET_ADDRESS");
            let socket = tokio::net::TcpStream::connect(socket)
                .await
                .expect("unable to connect to socket address");
            UsbmuxdConnection::new(Box::new(socket), 1)
        } else {
            UsbmuxdConnection::default()
                .await
                .expect("Unable to connect to usbmxud")
        };
        let devs = match usbmuxd.get_devices().await {
            Ok(d) => d,
            Err(e) => {
                return Err(format!("Unable to get devices from usbmuxd: {e:?}"));
            }
        };
        if devs.is_empty() {
            return Err("No devices connected!".to_string());
        }
        Box::new(devs[0].to_provider(UsbmuxdAddr::from_env_var().unwrap(), label))
    };
    Ok(provider)
}
