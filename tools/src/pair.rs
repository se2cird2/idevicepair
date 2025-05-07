// Complete the missing parts of the worker_loop function to handle all AFC commands

async fn worker_loop(rx: Receiver<Command>, tx: Sender<GuiEvent>) {
    // Create a cache of AFC clients to avoid recreating them for each operation
    let afc_clients: Arc<Mutex<HashMap<String, AfcClient>>> = Arc::new(Mutex::new(HashMap::new()));
    
    loop {
        match rx.recv() {
            Ok(Command::Refresh) => {
                let udids = match scan_devices().await {
                    Ok(list) => list,
                    Err(e) => { let _ = tx.send(GuiEvent::Status(format!("Error scanning: {e:?}"))); vec![] }
                };
                let mut devices = Vec::new();
                
                for udid in &udids {
                    let name = get_device_name(udid).await.unwrap_or_else(|_| udid.clone());
                    let model = get_device_model(udid).await.unwrap_or_else(|_| "".to_string());
                    let display = if model.is_empty() {
                        name.clone()
                    } else {
                        format!("{} ({})", name, model)
                    };
                    devices.push((udid.clone(), display));
                    
                    // Immediately fetch device info for this device
                    if let Ok(info) = get_device_info(udid).await {
                        let _ = tx.send(GuiEvent::DeviceInfo { udid: udid.clone(), info });
                    }
                }
                
                let _ = tx.send(GuiEvent::Devices(devices.clone()));
            }
            Ok(Command::Pair { udid, out_dir }) => {
                let _ = tx.send(GuiEvent::Status(format!("Pairing {udid}")));
                match pair_one(&out_dir, &udid).await {
                    Ok(dir_path) => {
                        let _ = tx.send(GuiEvent::Status(format!("Successfully paired {udid}")));
                        // Open the directory where the pair file was saved
                        reveal_in_file_browser(&dir_path);
                    },
                    Err(e) => { let _ = tx.send(GuiEvent::Status(format!("Error pairing {udid}: {e:?}"))); }
                }
            }
            Ok(Command::GetDeviceInfo { udid }) => {
                let _ = tx.send(GuiEvent::Status(format!("Getting info for {udid}")));
                match get_device_info(&udid).await {
                    Ok(info) => { let _ = tx.send(GuiEvent::DeviceInfo { udid, info }); }
                    Err(e) => { let _ = tx.send(GuiEvent::Status(format!("Error getting device info: {e:?}"))); }
                }
            }
            // AFC Commands
            Ok(Command::AfcListDir { udid, path }) => {
                let _ = tx.send(GuiEvent::Status(format!("Listing directory: {path}")));
                match get_afc_client(&udid, &afc_clients).await {
                    Ok(mut client) => {
                        match client.list_dir(&path).await {
                            Ok(entries) => {
                                let _ = tx.send(GuiEvent::AfcDirListing { path, entries });
                                
                                // Add client back to cache
                                let mut clients = afc_clients.lock().unwrap();
                                clients.insert(udid, client);
                            },
                            Err(e) => { 
                                let _ = tx.send(GuiEvent::Status(format!("Error listing directory: {e:?}"))); 
                            }
                        }
                    },
                    Err(e) => { let _ = tx.send(GuiEvent::Status(format!("Error connecting to AFC: {e:?}"))); }
                }
            }
            Ok(Command::AfcMkDir { udid, path }) => {
                let _ = tx.send(GuiEvent::Status(format!("Creating directory: {path}")));
                match get_afc_client(&udid, &afc_clients).await {
                    Ok(mut client) => {
                        match client.mk_dir(&path).await {
                            Ok(_) => {
                                let _ = tx.send(GuiEvent::AfcOperationResult { 
                                    operation: "Create Directory".to_string(),
                                    success: true,
                                    message: path
                                });
                                
                                // Add client back to cache
                                let mut clients = afc_clients.lock().unwrap();
                                clients.insert(udid, client);
                            },
                            Err(e) => { 
                                let _ = tx.send(GuiEvent::AfcOperationResult { 
                                    operation: "Create Directory".to_string(),
                                    success: false,
                                    message: format!("{e:?}") 
                                });
                            }
                        }
                    },
                    Err(e) => { let _ = tx.send(GuiEvent::Status(format!("Error connecting to AFC: {e:?}"))); }
                }
            }
            Ok(Command::AfcDownload { udid, path, save_path }) => {
                let _ = tx.send(GuiEvent::Status(format!("Downloading: {path}")));
                match get_afc_client(&udid, &afc_clients).await {
                    Ok(mut client) => {
                        match client.open(&path, AfcFopenMode::RdOnly).await {
                            Ok(mut file) => {
                                match file.read().await {
                                    Ok(data) => {
                                        match tokio::fs::write(&save_path, &data).await {
                                            Ok(_) => {
                                                let _ = tx.send(GuiEvent::AfcOperationResult { 
                                                    operation: "Download".to_string(),
                                                    success: true,
                                                    message: format!("Saved to {}", save_path.display())
                                                });
                                            },
                                            Err(e) => {
                                                let _ = tx.send(GuiEvent::AfcOperationResult { 
                                                    operation: "Download".to_string(),
                                                    success: false,
                                                    message: format!("Failed to write file: {e:?}")
                                                });
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        let _ = tx.send(GuiEvent::AfcOperationResult { 
                                            operation: "Download".to_string(),
                                            success: false,
                                            message: format!("Failed to read file: {e:?}")
                                        });
                                    }
                                }
                                
                                // Add client back to cache
                                let mut clients = afc_clients.lock().unwrap();
                                clients.insert(udid, client);
                            },
                            Err(e) => {
                                let _ = tx.send(GuiEvent::AfcOperationResult { 
                                    operation: "Download".to_string(),
                                    success: false,
                                    message: format!("Failed to open file: {e:?}")
                                });
                            }
                        }
                    },
                    Err(e) => { let _ = tx.send(GuiEvent::Status(format!("Error connecting to AFC: {e:?}"))); }
                }
            }
            Ok(Command::AfcUpload { udid, file_path, device_path }) => {
                let _ = tx.send(GuiEvent::Status(format!("Uploading to: {device_path}")));
                match get_afc_client(&udid, &afc_clients).await {
                    Ok(mut client) => {
                        match tokio::fs::read(&file_path).await {
                            Ok(bytes) => {
                                match client.open(&device_path, AfcFopenMode::WrOnly).await {
                                    Ok(mut file) => {
                                        match file.write(&bytes).await {
                                            Ok(_) => {
                                                let _ = tx.send(GuiEvent::AfcOperationResult { 
                                                    operation: "Upload".to_string(),
                                                    success: true,
                                                    message: device_path
                                                });
                                            },
                                            Err(e) => {
                                                let _ = tx.send(GuiEvent::AfcOperationResult { 
                                                    operation: "Upload".to_string(),
                                                    success: false,
                                                    message: format!("Failed to write to device: {e:?}")
                                                });
                                            }
                                        }
                                        
                                        // Add client back to cache
                                        let mut clients = afc_clients.lock().unwrap();
                                        clients.insert(udid, client);
                                    },
                                    Err(e) => {
                                        let _ = tx.send(GuiEvent::AfcOperationResult { 
                                            operation: "Upload".to_string(),
                                            success: false,
                                            message: format!("Failed to open file on device: {e:?}")
                                        });
                                    }
                                }
                            },
                            Err(e) => {
                                let _ = tx.send(GuiEvent::AfcOperationResult { 
                                    operation: "Upload".to_string(),
                                    success: false,
                                    message: format!("Failed to read local file: {e:?}")
                                });
                            }
                        }
                    },
                    Err(e) => { let _ = tx.send(GuiEvent::Status(format!("Error connecting to AFC: {e:?}"))); }
                }
            }
            Ok(Command::AfcRemove { udid, path }) => {
                let _ = tx.send(GuiEvent::Status(format!("Deleting: {path}")));
                match get_afc_client(&udid, &afc_clients).await {
                    Ok(mut client) => {
                        match client.remove(&path).await {
                            Ok(_) => {
                                let _ = tx.send(GuiEvent::AfcOperationResult { 
                                    operation: "Delete".to_string(),
                                    success: true,
                                    message: path
                                });
                                
                                // Add client back to cache
                                let mut clients = afc_clients.lock().unwrap();
                                clients.insert(udid, client);
                            },
                            Err(e) => {
                                // Try remove_all which works for directories
                                match client.remove_all(&path).await {
                                    Ok(_) => {
                                        let _ = tx.send(GuiEvent::AfcOperationResult { 
                                            operation: "Delete".to_string(),
                                            success: true,
                                            message: format!("Recursively deleted {}", path)
                                        });
                                        
                                        // Add client back to cache
                                        let mut clients = afc_clients.lock().unwrap();
                                        clients.insert(udid, client);
                                    },
                                    Err(e2) => {
                                        let _ = tx.send(GuiEvent::AfcOperationResult { 
                                            operation: "Delete".to_string(),
                                            success: false,
                                            message: format!("Failed to delete: {e:?} (recursive: {e2:?})")
                                        });
                                    }
                                }
                            }
                        }
                    },
                    Err(e) => { let _ = tx.send(GuiEvent::Status(format!("Error connecting to AFC: {e:?}"))); }
                }
            }
            Ok(Command::AfcGetFileInfo { udid, path }) => {
                let _ = tx.send(GuiEvent::Status(format!("Getting info for: {path}")));
                match get_afc_client(&udid, &afc_clients).await {
                    Ok(mut client) => {
                        match client.get_file_info(&path).await {
                            Ok(info) => {
                                // Convert to hashmap of strings
                                let string_info: HashMap<String, String> = info.into_iter()
                                    .map(|(k, v)| (k, v.to_string()))
                                    .collect();
                                
                                let _ = tx.send(GuiEvent::AfcFileInfo { 
                                    path,
                                    info: string_info
                                });
                                
                                // Add client back to cache
                                let mut clients = afc_clients.lock().unwrap();
                                clients.insert(udid, client);
                            },
                            Err(e) => {
                                let _ = tx.send(GuiEvent::Status(format!("Failed to get file info: {e:?}")));
                            }
                        }
                    },
                    Err(e) => { let _ = tx.send(GuiEvent::Status(format!("Error connecting to AFC: {e:?}"))); }
                }
            }
            Ok(Command::AfcGetDeviceInfo { udid }) => {
                let _ = tx.send(GuiEvent::Status("Getting AFC device info...".to_string()));
                match get_afc_client(&udid, &afc_clients).await {
                    Ok(mut client) => {
                        match client.get_device_info().await {
                            Ok(info) => {
                                // Convert to hashmap of strings
                                let string_info: HashMap<String, String> = info.into_iter()
                                    .map(|(k, v)| (k, v.to_string()))
                                    .collect();
                                
                                let _ = tx.send(GuiEvent::AfcDeviceInfo { 
                                    info: string_info
                                });
                                
                                // Add client back to cache
                                let mut clients = afc_clients.lock().unwrap();
                                clients.insert(udid, client);
                            },
                            Err(e) => {
                                let _ = tx.send(GuiEvent::Status(format!("Failed to get AFC device info: {e:?}")));
                            }
                        }
                    },
                    Err(e) => { let _ = tx.send(GuiEvent::Status(format!("Error connecting to AFC: {e:?}"))); }
                }
            }
            Err(e) => {
                eprintln!("Worker channel error: {e:?}");
                break;
            }
        }
    }
}

// Helper function to get or create an AFC client for a device
async fn get_afc_client(
    udid: &str, 
    cache: &Arc<Mutex<HashMap<String, AfcClient>>>
) -> Result<AfcClient, Box<dyn std::error::Error>> {
    // Check if we have a cached client
    {
        let clients = cache.lock().unwrap();
        if let Some(client) = clients.get(udid) {
            return Ok(client.clone());
        }
    }
    
    // No cached client, create a new one
    let provider = common::get_provider(
        Some(&udid.to_string()), 
        None, 
        None, 
        "afc-pair-gui"
    ).await?;
    
    let client = AfcClient::connect(&*provider).await?;
    Ok(client)
}

// Helper function to reveal a file/directory in the OS file browser
fn reveal_in_file_browser(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = SysCmd::new("explorer")
            .args(["/select,", &path.to_string_lossy()])
            .spawn();
    }
    
    #[cfg(target_os = "macos")]
    {
        let _ = SysCmd::new("open")
            .args(["-R", &path.to_string_lossy()])
            .spawn();
    }
    
    #[cfg(target_os = "linux")]
    {
        if let Some(parent) = path.parent() {
            // Try common file managers
            for cmd in &["xdg-open", "nautilus", "dolphin", "thunar", "pcmanfm"] {
                if SysCmd::new(cmd)
                    .arg(parent.to_string_lossy().to_string())
                    .spawn()
                    .is_ok() {
                    break;
                }
            }
        }
    }
}

// Add a utility function to scan for devices
async fn scan_devices() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let connection = UsbmuxdConnection::default();
    let devices = connection.get_devices().await?;
    Ok(devices.into_iter().map(|d| d.udid).collect())
}

// Get device name
async fn get_device_name(udid: &str) -> Result<String, Box<dyn std::error::Error>> {
    let provider = common::get_provider(
        Some(&udid.to_string()), 
        None, 
        None, 
        "lockdown-info"
    ).await?;
    
    let client = LockdownClient::connect(&*provider).await?;
    let name = client.get_device_name().await?;
    Ok(name)
}

// Get device model
async fn get_device_model(udid: &str) -> Result<String, Box<dyn std::error::Error>> {
    let provider = common::get_provider(
        Some(&udid.to_string()), 
        None, 
        None, 
        "lockdown-info"
    ).await?;
    
    let client = LockdownClient::connect(&*provider).await?;
    if let Ok(value) = client.get_value("", "ProductType").await {
        if let Some(Value::String(product_type)) = value {
            return Ok(product_type);
        }
    }
    
    Ok("Unknown".to_string())
}

// Get detailed device info
async fn get_device_info(udid: &str) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let provider = common::get_provider(
        Some(&udid.to_string()), 
        None, 
        None, 
        "lockdown-info"
    ).await?;
    
    let client = LockdownClient::connect(&*provider).await?;
    let value = client.get_value("", "").await?;
    
    fn extract_values(value: &Value, prefix: &str) -> HashMap<String, String> {
        let mut result = HashMap::new();
        
        match value {
            Value::Dictionary(dict) => {
                for (key, val) in dict {
                    match val {
                        Value::String(s) => {
                            let full_key = if prefix.is_empty() {
                                key.clone()
                            } else {
                                format!("{}.{}", prefix, key)
                            };
                            result.insert(full_key, s.clone());
                        }
                        Value::Integer(i) => {
                            let full_key = if prefix.is_empty() {
                                key.clone()
                            } else {
                                format!("{}.{}", prefix, key)
                            };
                            result.insert(full_key, i.to_string());
                        }
                        Value::Real(r) => {
                            let full_key = if prefix.is_empty() {
                                key.clone()
                            } else {
                                format!("{}.{}", prefix, key)
                            };
                            result.insert(full_key, r.to_string());
                        }
                        Value::Boolean(b) => {
                            let full_key = if prefix.is_empty() {
                                key.clone()
                            } else {
                                format!("{}.{}", prefix, key)
                            };
                            result.insert(full_key, b.to_string());
                        }
                        Value::Date(d) => {
                            let full_key = if prefix.is_empty() {
                                key.clone()
                            } else {
                                format!("{}.{}", prefix, key)
                            };
                            result.insert(full_key, format!("{:?}", d));
                        }
                        Value::Data(d) => {
                            let full_key = if prefix.is_empty() {
                                key.clone()
                            } else {
                                format!("{}.{}", prefix, key)
                            };
                            result.insert(full_key, format!("<{} bytes>", d.len()));
                        }
                        Value::Dictionary(_) | Value::Array(_) => {
                            let new_prefix = if prefix.is_empty() {
                                key.clone()
                            } else {
                                format!("{}.{}", prefix, key)
                            };
                            let nested = extract_values(val, &new_prefix);
                            result.extend(nested);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        
        result
    }
    
    Ok(extract_values(&value, ""))
}

// Pairing function
async fn pair_one(out_dir: &Path, udid: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let provider = common::get_provider(
        Some(&udid.to_string()), 
        None, 
        None, 
        "lockdown-info"
    ).await?;
    
    let client = LockdownClient::connect(&*provider).await?;
    
    // Create a pairing record
    let pair_record = client.pair().await?;
    
    // Generate a UUID for the file
    let id = Uuid::new_v4();
    
    // Create output directory if it doesn't exist
    fs::create_dir_all(out_dir)?;
    
    // Save to file
    let file_path = out_dir.join(format!("{}.plist", id));
    let file = std::fs::File::create(&file_path)?;
    
    plist::to_writer_xml(file, &pair_record)?;
    
    Ok(file_path)
}

// Main function to launch the app
fn main() -> Result<(), eframe::Error> {
    env_logger::init();
    
    // Load preferences for default directories
    let prefs = load_prefs();
    let default_dir = prefs.output_dir.unwrap_or_else(|| {
        if let Some(base_dirs) = BaseDirs::new() {
            base_dirs.download_dir().to_path_buf()
        } else {
            PathBuf::from(".")
        }
    });
    
    // Create channels for communication between GUI and worker
    let (tx_cmd, rx_cmd) = unbounded();
    let (tx_gui, rx_gui) = unbounded();
    
    // Spawn worker thread
    thread::spawn(move || {
        let rt = Runtime::new().unwrap();
        rt.block_on(worker_loop(rx_cmd, tx_gui));
    });
    
    // Launch the GUI
    let app = PairApp::new(tx_cmd.clone(), rx_gui, default_dir, prefs.last_afc_path);
    
    let native_options = NativeOptions {
        initial_window_size: Some(egui::vec2(800.0, 600.0)),
        ..Default::default()
    };
    
    // Send initial refresh command
    let _ = tx_cmd.send(Command::Refresh);
    
    eframe::run_native("iOS Device Manager", native_options, Box::new(|_cc| Box::new(app)))
}