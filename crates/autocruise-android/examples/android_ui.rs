//! Developer helper for writing app connectors: prints the current screen's
//! UI tree, optionally after tapping a node.
//!
//! cargo run -p autocruise-android --example android_ui
//! cargo run -p autocruise-android --example android_ui -- tap-text "Continue with email"
//! cargo run -p autocruise-android --example android_ui -- back
//!
//! Uses AUTOCRUISE_ANDROID_SERIAL (default 127.0.0.1:5555).

use std::time::Duration;

use autocruise_android::{By, Device, Key};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = Device::from_env();
    device.ensure_ready().await?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        [] => {}
        ["tap-text", text] => {
            let tree = device.ui().await?;
            let node = tree
                .find(&By::text(text))
                .or_else(|| tree.find(&By::desc(text)))
                .ok_or_else(|| format!("no node with text or description {text:?}"))?;
            device.tap(node).await?;
        }
        ["tap-id", id] => {
            let tree = device.ui().await?;
            let node = tree
                .find(&By::id(id))
                .ok_or_else(|| format!("no node {id}"))?;
            device.tap(node).await?;
        }
        ["back"] => device.press(Key::Back).await?,
        ["launch", package] => device.launch(package).await?,
        _ => {
            return Err(
                "usage: android_ui [tap-text TEXT | tap-id ID | back | launch PACKAGE]".into(),
            );
        }
    }
    if !args.is_empty() {
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    print!("{}", device.ui().await?);
    Ok(())
}
