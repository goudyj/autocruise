//! Prints the accessibility tree of an application as JSON (fixture format).
//!
//! ```sh
//! AUTOCRUISE_DESKTOP_DISPLAY=:99 AUTOCRUISE_DESKTOP_DBUS=unix:path=/path/to/bus \
//!   cargo run -p autocruise-desktop --example dump -- chrome > tree.json
//! ```
//!
//! Recorded trees contain whatever the screen shows: sanitize them (names,
//! e-mail addresses, card numbers) before committing them as fixtures.

use autocruise_desktop::Desktop;
use autocruise_desktop::linux::LinuxDesktop;

#[tokio::main]
async fn main() {
    let app = std::env::args().nth(1).unwrap_or_else(|| "chrome".into());
    let desktop = LinuxDesktop::from_env();
    let start = std::time::Instant::now();
    match desktop.tree(&app).await {
        Ok(tree) => {
            eprintln!("{} nodes in {:?}", tree.node_count(), start.elapsed());
            println!("{}", serde_json::to_string(&tree).expect("serializable"));
        }
        Err(e) => {
            eprintln!("error: {e}");
            if let Ok(apps) = desktop.applications().await {
                eprintln!("applications: {apps:?}");
            }
            std::process::exit(1);
        }
    }
}
