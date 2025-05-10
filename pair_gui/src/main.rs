// src/main.rs

mod types;
mod prefs;
mod util;
mod ui;
mod worker;

// add this:
use worker::worker_loop::worker_loop;

use prefs::load_prefs;
use util::canonical_or_create;
use crossbeam::channel::unbounded;
use tokio::runtime::Runtime;
use eframe::{run_native, NativeOptions};
use ui::app::PairApp;

fn main() -> eframe::Result<()> {
    env_logger::init();
    let prefs = load_prefs();
    let default_dir = prefs
        .output_dir
        .clone()
        .unwrap_or_else(|| canonical_or_create("pairings"));
    let (tx_cmd, rx_cmd) = unbounded();
    let (tx_evt, rx_evt) = unbounded();

    std::thread::spawn(move || {
        let rt = Runtime::new().unwrap();
        // call the function, not the module
        rt.block_on(worker_loop(rx_cmd, tx_evt));
    });

    let app = PairApp::new(tx_cmd, rx_evt, default_dir);
    run_native("iOS Pair Utility", NativeOptions::default(), Box::new(|_| Ok(Box::new(app))))
}
