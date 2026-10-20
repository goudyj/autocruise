//! Desktop access for autocruise: the user's real application, read through
//! accessibility and driven with real input, at human pace.
//!
//! Some sites block any browser driven over a debugging protocol (CDP). This
//! crate reaches the user's own, normal browser the way assistive technology
//! does instead:
//!
//! - **Read**: the accessibility tree (AT-SPI on Linux): roles, names,
//!   values, bounds and states. Nothing is injected into the page.
//! - **Act**: real pointer and keyboard events on the display (X11 XTest),
//!   exactly like a person at the keyboard.
//!
//! The crate names no service. Connectors use a [`Driver`], which paces every
//! input with [`autocruise_core::throttle`] and polls the tree until a page is
//! ready.
//!
//! - [`Desktop`]: the transport trait. [`linux::LinuxDesktop`] in production,
//!   [`mock::MockDesktop`] in tests (recorded trees as fixtures, records inputs).
//! - [`Node`]: an owned snapshot of an accessibility tree.
//! - [`DesktopError`]: failures, classified with [`autocruise_core::error::Classify`].

mod driver;
mod error;
pub mod keys;
pub mod linux;
pub mod mock;
mod node;
pub mod record;

use std::fmt;

pub use autocruise_core::http::BoxFuture;
pub use driver::{Check, Driver};
pub use error::DesktopError;
pub use node::{Node, Rect};

/// One real input event sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// Move the pointer to a screen point and left-click.
    Click { x: i32, y: i32 },
    /// Type text as key presses (layout-independent).
    Type(String),
    /// Press a key combination, e.g. `"Return"`, `"ctrl+a"`.
    Key(String),
    /// Move the pointer to a screen point and turn the wheel: positive
    /// `notches` scroll down, negative scroll up.
    Scroll { x: i32, y: i32, notches: i32 },
}

/// Access to the user's desktop.
pub trait Desktop: Send + Sync + fmt::Debug {
    /// Names of the applications exposed on the accessibility bus.
    fn applications(&self) -> BoxFuture<'_, Result<Vec<String>, DesktopError>>;

    /// Accessibility tree of the first application whose name contains
    /// `app` (case-insensitive). Its children are the windows.
    fn tree<'a>(&'a self, app: &'a str) -> BoxFuture<'a, Result<Node, DesktopError>>;

    /// Sends real input to the display.
    fn input(&self, input: Input) -> BoxFuture<'_, Result<(), DesktopError>>;
}
