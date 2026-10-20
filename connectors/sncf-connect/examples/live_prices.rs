//! Manual live check: one price search in the user's browser, optionally
//! recording every accessibility tree read (to build test fixtures).
//!
//! ```sh
//! AUTOCRUISE_DESKTOP_DISPLAY=:99 AUTOCRUISE_DESKTOP_DBUS=unix:path=/path/to/bus \
//!   cargo run -p autocruise-sncf-connect --example live_prices -- \
//!   '{"from":"Paris","to":"Lyon","date":"2026-10-21","time_after":"08:00"}' [record-dir]
//! ```
//!
//! Recorded trees contain personal data shown on screen (name, companions):
//! sanitize before committing anything under tests/fixtures.

use std::sync::Arc;

use autocruise_desktop::linux::LinuxDesktop;
use autocruise_desktop::record::{Event, RecordingDesktop};
use autocruise_sncf_connect::SncfConnect;
use autocruise_sncf_connect::types::PricesParams;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let params: PricesParams =
        serde_json::from_str(args.get(1).expect("JSON params")).expect("valid params");
    let record_dir = args.get(2).cloned();
    let desktop = Arc::new(RecordingDesktop::new(LinuxDesktop::from_env()));
    let service = SncfConnect::new(desktop.clone());
    let start = std::time::Instant::now();
    let result = service.prices(params).await;
    eprintln!("took {:?}", start.elapsed());
    if let Some(dir) = record_dir {
        std::fs::create_dir_all(&dir).expect("record dir");
        for (i, event) in desktop.events().iter().enumerate() {
            match event {
                Event::Tree(t) => std::fs::write(
                    format!("{dir}/{i:03}-tree.json"),
                    serde_json::to_string(t).expect("json"),
                )
                .expect("write"),
                Event::Input(input) => {
                    std::fs::write(format!("{dir}/{i:03}-input.txt"), format!("{input:?}"))
                        .expect("write")
                }
            }
        }
    }
    match result {
        Ok(r) => println!("{}", serde_json::to_string_pretty(&r).expect("json")),
        Err(e) => {
            eprintln!(
                "error ({}): {e}",
                autocruise_core::error::Classify::kind(&e).as_str()
            );
            std::process::exit(1);
        }
    }
}
