// Jackson Coxson

use std::{
    io::Write,
    net::{IpAddr, SocketAddr},
    str::FromStr,
};

use clap::{Arg, Command};
use idevice::{
    core_device_proxy::CoreDeviceProxy, debug_proxy::DebugProxyClient,
    tunneld::get_tunneld_devices, xpc::XPCDevice, IdeviceService, ReadWrite,
};
use tokio::net::TcpStream;

mod common;

#[tokio::main]
async fn main() {
    env_logger::init();

    let matches = Command::new("remotexpc")
        .about("Get services from RemoteXPC")
        .arg(
            Arg::new("host")
                .long("host")
                .value_name("HOST")
                .help("IP address of the device"),
        )
        .arg(
            Arg::new("pairing_file")
                .long("pairing-file")
                .value_name("PATH")
                .help("Path to the pairing file"),
        )
        .arg(
            Arg::new("udid")
                .value_name("UDID")
                .help("UDID of the device (overrides host/pairing file)")
                .index(1),
        )
        .arg(
            Arg::new("tunneld")
                .long("tunneld")
                .help("Use tunneld")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("about")
                .long("about")
                .help("Show about information")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("debug_proxy - connect to the debug proxy and run commands");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let pairing_file = matches.get_one::<String>("pairing_file");
    let host = matches.get_one::<String>("host");

    let mut dp: DebugProxyClient<Box<dyn ReadWrite>> = if matches.get_flag("tunneld") {
        let socket = SocketAddr::new(
            IpAddr::from_str("127.0.0.1").unwrap(),
            idevice::tunneld::DEFAULT_PORT,
        );
        let mut devices = get_tunneld_devices(socket)
            .await
            .expect("Failed to get tunneld devices");

        let (_udid, device) = match udid {
            Some(u) => (
                u.to_owned(),
                devices.remove(u).expect("Device not in tunneld"),
            ),
            None => devices.into_iter().next().expect("No devices"),
        };

        // Make the connection to RemoteXPC
        let client = XPCDevice::new(Box::new(
            TcpStream::connect((device.tunnel_address.as_str(), device.tunnel_port))
                .await
                .unwrap(),
        ))
        .await
        .unwrap();

        // Get the debug proxy
        let service = client
            .services
            .get(idevice::debug_proxy::SERVICE_NAME)
            .expect("Client did not contain debug proxy service");

        let stream = TcpStream::connect(SocketAddr::new(
            IpAddr::from_str(&device.tunnel_address).unwrap(),
            service.port,
        ))
        .await
        .expect("Failed to connect");

        DebugProxyClient::new(Box::new(stream))
    } else {
        let provider =
            match common::get_provider(udid, host, pairing_file, "debug-proxy-jkcoxson").await {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("{e}");
                    return;
                }
            };
        let proxy = CoreDeviceProxy::connect(&*provider)
            .await
            .expect("no core proxy");
        let rsd_port = proxy.handshake.server_rsd_port;

        let mut adapter = proxy.create_software_tunnel().expect("no software tunnel");
        adapter.connect(rsd_port).await.expect("no RSD connect");

        // Make the connection to RemoteXPC
        let client = XPCDevice::new(Box::new(adapter)).await.unwrap();

        // Get the debug proxy
        let service = client
            .services
            .get(idevice::debug_proxy::SERVICE_NAME)
            .expect("Client did not contain debug proxy service")
            .to_owned();

        let mut adapter = client.into_inner();
        adapter.close().await.unwrap();
        adapter.connect(service.port).await.unwrap();

        DebugProxyClient::new(Box::new(adapter))
    };

    println!("Shell connected!");
    loop {
        print!("> ");
        std::io::stdout().flush().unwrap();

        let mut buf = String::new();
        std::io::stdin().read_line(&mut buf).unwrap();

        let buf = buf.trim();

        if buf == "exit" {
            break;
        }

        let res = dp.send_command(buf.into()).await.expect("Failed to send");
        if let Some(res) = res {
            println!("{res}");
        }
    }
}
