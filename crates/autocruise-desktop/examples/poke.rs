//! Manual exploration of an application through the same paths autocruise
//! uses: `text` prints visible names (role, name, bounds), `click X Y`,
//! `key COMBO`, `type TEXT`, `scroll X Y NOTCHES`.
//!
//! ```sh
//! AUTOCRUISE_DESKTOP_DISPLAY=:50 AUTOCRUISE_DESKTOP_DBUS=unix:path=... \
//!   cargo run -p autocruise-desktop --example poke -- text chrome
//! ```

use autocruise_desktop::linux::LinuxDesktop;
use autocruise_desktop::{Desktop, Input, Node};

fn print(node: &Node, depth: usize) {
    if !node.name.is_empty() || node.value.is_some() {
        let b = node
            .bounds
            .map(|b| format!("[{},{},{},{}]", b.x, b.y, b.width, b.height));
        println!(
            "{}{}: {:?}{} {} {}",
            "  ".repeat(depth.min(20)),
            node.role,
            node.name,
            node.value
                .as_ref()
                .map(|v| format!(" = {v:?}"))
                .unwrap_or_default(),
            b.unwrap_or_default(),
            node.states.join(",")
        );
    }
    for c in &node.children {
        print(c, depth + 1);
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let desktop = LinuxDesktop::from_env();
    let int = |i: usize| args[i].parse::<i32>().expect("number");
    let result = match args.first().map(String::as_str) {
        Some("text") => desktop
            .tree(args.get(1).map_or("chrome", String::as_str))
            .await
            .map(|t| print(&t, 0)),
        Some("click") => {
            desktop
                .input(Input::Click {
                    x: int(1),
                    y: int(2),
                })
                .await
        }
        Some("key") => desktop.input(Input::Key(args[1].clone())).await,
        Some("type") => desktop.input(Input::Type(args[1].clone())).await,
        Some("scroll") => {
            desktop
                .input(Input::Scroll {
                    x: int(1),
                    y: int(2),
                    notches: int(3),
                })
                .await
        }
        _ => {
            eprintln!("usage: poke text [app] | click X Y | key COMBO | type TEXT | scroll X Y N");
            std::process::exit(2);
        }
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
