//! SNCF Connect connector for autocruise (**unofficial**).
//!
//! SNCF Connect has no public price API and its website is protected by
//! DataDome, which blocks browsers driven over a debugging protocol (CDP).
//! This connector therefore works like a person at the user's computer:
//!
//! - it uses the user's **own, normal Chrome**, already logged in by the user;
//! - it reads pages through **accessibility** (AT-SPI) and acts with **real
//!   desktop input** (X11 XTest), at human pace, through
//!   [`autocruise_desktop::Driver`];
//! - on a bot check, a block page or a logged-out session it **stops** and
//!   returns `user_action_required`: it never interacts with a challenge.
//!
//! Layout:
//! - [`types`]: tool inputs and outputs.
//! - `page`: pure functions reading accessibility trees (detection, parsing).
//! - `service`: [`SncfConnect`], one async method per tool (drives the browser).
//! - `error`: [`SncfConnectError`].

mod error;
mod page;
mod service;
pub mod types;

pub use error::SncfConnectError;
pub use service::{SncfConnect, Timing};
