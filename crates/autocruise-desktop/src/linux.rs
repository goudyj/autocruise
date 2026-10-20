//! Production desktop on Linux: AT-SPI (over D-Bus) to read, X11 XTest to act.
//!
//! The target is a display (`:99`) and the D-Bus session bus of that
//! desktop, which advertises its accessibility bus. Nothing here talks to the
//! browser's debugging protocol.

use std::time::Duration;

use atspi::CoordType;
use atspi::proxy::accessible::AccessibleProxy;
use atspi::proxy::bus::BusProxy;
use atspi::proxy::component::ComponentProxy;
use atspi::proxy::text::TextProxy;
use atspi::zbus;
use tokio::sync::Mutex;
use x11rb::connection::Connection as _;
use x11rb::protocol::xproto::{self, ConnectionExt as _};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

use crate::keys::{self, SHIFT_L};
use crate::node::KEPT_STATES;
use crate::{BoxFuture, Desktop, DesktopError, Input, Node, Rect};

const REGISTRY: &str = "org.a11y.atspi.Registry";
const ROOT_PATH: &str = "/org/a11y/atspi/accessible/root";
/// Upper bound on nodes read per snapshot.
const MAX_NODES: usize = 8000;
const MAX_DEPTH: usize = 80;

/// Where the desktop is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinuxConfig {
    /// X display, e.g. `:99`. Defaults to `$DISPLAY`.
    pub display: Option<String>,
    /// D-Bus session bus address of that desktop (used to find its
    /// accessibility bus). Defaults to `$DBUS_SESSION_BUS_ADDRESS`.
    pub session_bus: Option<String>,
    /// Accessibility bus address, when known directly (`$AT_SPI_BUS_ADDRESS`).
    pub a11y_bus: Option<String>,
}

impl LinuxConfig {
    /// `AUTOCRUISE_DESKTOP_DISPLAY` (else `DISPLAY`),
    /// `AUTOCRUISE_DESKTOP_DBUS` (else `DBUS_SESSION_BUS_ADDRESS`),
    /// `AT_SPI_BUS_ADDRESS`.
    pub fn from_env() -> Self {
        let var = |names: &[&str]| {
            names
                .iter()
                .find_map(|n| std::env::var(n).ok().filter(|v| !v.trim().is_empty()))
        };
        Self {
            display: var(&["AUTOCRUISE_DESKTOP_DISPLAY", "DISPLAY"]),
            session_bus: var(&["AUTOCRUISE_DESKTOP_DBUS", "DBUS_SESSION_BUS_ADDRESS"]),
            a11y_bus: var(&["AT_SPI_BUS_ADDRESS"]),
        }
    }
}

/// Real desktop. Connections are opened lazily and reopened after a failure,
/// so a desktop that restarts is picked up again.
#[derive(Debug)]
pub struct LinuxDesktop {
    config: LinuxConfig,
    keystroke_delay: Duration,
    a11y: Mutex<Option<zbus::Connection>>,
    x11: Mutex<Option<X11Input>>,
}

impl LinuxDesktop {
    pub fn new(config: LinuxConfig) -> Self {
        Self {
            config,
            keystroke_delay: Duration::from_millis(70),
            a11y: Mutex::new(None),
            x11: Mutex::new(None),
        }
    }

    pub fn from_env() -> Self {
        Self::new(LinuxConfig::from_env())
    }

    pub fn config(&self) -> &LinuxConfig {
        &self.config
    }

    async fn a11y_connection(&self) -> Result<zbus::Connection, DesktopError> {
        let mut guard = self.a11y.lock().await;
        if let Some(conn) = guard.as_ref() {
            return Ok(conn.clone());
        }
        let conn = connect_a11y(&self.config).await?;
        *guard = Some(conn.clone());
        Ok(conn)
    }

    async fn read_tree(&self, app: &str) -> Result<Node, DesktopError> {
        let conn = self.a11y_connection().await?;
        let result = find_app(&conn, app).await;
        if matches!(result, Err(DesktopError::Unavailable(_))) {
            // The bus went away (desktop restarted): reconnect next time.
            *self.a11y.lock().await = None;
        }
        let (dest, path) = result?;
        let mut budget = MAX_NODES;
        read_node(&conn, &dest, &path, 0, &mut budget)
            .await
            .ok_or_else(|| DesktopError::AppNotFound(app.to_owned()))
    }

    async fn send(&self, input: Input) -> Result<(), DesktopError> {
        let mut guard = self.x11.lock().await;
        if guard.is_none() {
            *guard = Some(X11Input::connect(self.config.display.as_deref())?);
        }
        let x = guard.as_mut().expect("connected above");
        let result = x.send(&input, self.keystroke_delay).await;
        if matches!(result, Err(DesktopError::Unavailable(_))) {
            *guard = None;
        }
        result
    }
}

impl Desktop for LinuxDesktop {
    fn applications(&self) -> BoxFuture<'_, Result<Vec<String>, DesktopError>> {
        Box::pin(async move {
            let conn = self.a11y_connection().await?;
            let root = accessible(&conn, REGISTRY, ROOT_PATH).await?;
            let mut names = Vec::new();
            for child in root.get_children().await.map_err(unavailable)? {
                let (Some(dest), path) = (child.name_as_str(), child.path_as_str()) else {
                    continue;
                };
                if let Ok(app) = accessible(&conn, dest, path).await
                    && let Ok(name) = app.name().await
                {
                    names.push(name);
                }
            }
            Ok(names)
        })
    }

    fn tree<'a>(&'a self, app: &'a str) -> BoxFuture<'a, Result<Node, DesktopError>> {
        Box::pin(self.read_tree(app))
    }

    fn input(&self, input: Input) -> BoxFuture<'_, Result<(), DesktopError>> {
        Box::pin(self.send(input))
    }
}

fn unavailable(e: impl std::fmt::Display) -> DesktopError {
    DesktopError::Unavailable(e.to_string())
}

async fn connect_a11y(config: &LinuxConfig) -> Result<zbus::Connection, DesktopError> {
    let a11y_address = match &config.a11y_bus {
        Some(address) => address.clone(),
        None => {
            let session = config
                .session_bus
                .as_deref()
                .ok_or_else(|| unavailable("no D-Bus session bus address configured"))?;
            let bus = zbus::connection::Builder::address(session)
                .map_err(unavailable)?
                .build()
                .await
                .map_err(unavailable)?;
            BusProxy::new(&bus)
                .await
                .map_err(unavailable)?
                .get_address()
                .await
                .map_err(|e| unavailable(format!("accessibility bus not found: {e}")))?
        }
    };
    zbus::connection::Builder::address(a11y_address.as_str())
        .map_err(unavailable)?
        .build()
        .await
        .map_err(unavailable)
}

async fn accessible<'a>(
    conn: &zbus::Connection,
    dest: &'a str,
    path: &'a str,
) -> Result<AccessibleProxy<'a>, DesktopError> {
    AccessibleProxy::builder(conn)
        .destination(dest)
        .and_then(|b| b.path(path))
        .map_err(|e| DesktopError::Protocol(e.to_string()))?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .map_err(|e| DesktopError::Protocol(e.to_string()))
}

/// Bus name and path of the first application whose name contains `app`.
async fn find_app(conn: &zbus::Connection, app: &str) -> Result<(String, String), DesktopError> {
    let root = accessible(conn, REGISTRY, ROOT_PATH).await?;
    let wanted = app.to_lowercase();
    for child in root.get_children().await.map_err(unavailable)? {
        let (Some(dest), path) = (child.name_as_str(), child.path_as_str()) else {
            continue;
        };
        let Ok(proxy) = accessible(conn, dest, path).await else {
            continue;
        };
        if let Ok(name) = proxy.name().await
            && name.to_lowercase().contains(&wanted)
        {
            return Ok((dest.to_owned(), path.to_owned()));
        }
    }
    Err(DesktopError::AppNotFound(app.to_owned()))
}

/// Reads one object and its subtree. Objects that vanish while reading
/// (pages change) are skipped.
fn read_node<'a>(
    conn: &'a zbus::Connection,
    dest: &'a str,
    path: &'a str,
    depth: usize,
    budget: &'a mut usize,
) -> BoxFuture<'a, Option<Node>> {
    Box::pin(async move {
        if *budget == 0 || depth > MAX_DEPTH {
            return None;
        }
        *budget -= 1;
        let acc = accessible(conn, dest, path).await.ok()?;
        let (role, name, states, children) = tokio::join!(
            acc.get_role_name(),
            acc.name(),
            acc.get_state(),
            acc.get_children()
        );
        let role = role.ok()?;
        let states: Vec<String> = states
            .map(|set| {
                set.iter()
                    .map(|s| s.to_static_str())
                    .filter(|s| KEPT_STATES.contains(s))
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        let bounds = extents(conn, dest, path).await;
        let value = if wants_value(&role, &states) {
            text_value(conn, dest, path).await
        } else {
            None
        };
        let mut node = Node {
            role,
            name: name.unwrap_or_default(),
            value,
            bounds,
            states,
            children: Vec::new(),
        };
        for child in children.unwrap_or_default() {
            let Some(child_dest) = child.name_as_str() else {
                continue;
            };
            if let Some(n) =
                read_node(conn, child_dest, child.path_as_str(), depth + 1, budget).await
            {
                node.children.push(n);
            }
        }
        Some(node)
    })
}

fn wants_value(role: &str, states: &[String]) -> bool {
    // Never read password fields.
    role != "password text"
        && (matches!(role, "entry" | "combo box" | "spin button" | "text")
            || states.iter().any(|s| s == "editable"))
}

async fn extents(conn: &zbus::Connection, dest: &str, path: &str) -> Option<Rect> {
    let proxy = ComponentProxy::builder(conn)
        .destination(dest)
        .ok()?
        .path(path)
        .ok()?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .ok()?;
    let (x, y, w, h) = proxy.get_extents(CoordType::Screen).await.ok()?;
    Some(Rect::new(x, y, w, h))
}

async fn text_value(conn: &zbus::Connection, dest: &str, path: &str) -> Option<String> {
    let proxy = TextProxy::builder(conn)
        .destination(dest)
        .ok()?
        .path(path)
        .ok()?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .ok()?;
    let text = proxy.get_text(0, -1).await.ok()?;
    // U+FFFC stands for embedded child objects.
    let text: String = text.chars().filter(|c| *c != '\u{fffc}').collect();
    Some(text.trim().to_owned())
}

/// XTest input on one display.
struct X11Input {
    conn: RustConnection,
    root: xproto::Window,
    min_keycode: u8,
    per_keycode: usize,
    keymap: Vec<u32>,
}

impl std::fmt::Debug for X11Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("X11Input")
            .field("root", &self.root)
            .finish()
    }
}

const KEY_PRESS: u8 = xproto::KEY_PRESS_EVENT;
const KEY_RELEASE: u8 = xproto::KEY_RELEASE_EVENT;
const BUTTON_PRESS: u8 = xproto::BUTTON_PRESS_EVENT;
const BUTTON_RELEASE: u8 = xproto::BUTTON_RELEASE_EVENT;
const MOTION: u8 = xproto::MOTION_NOTIFY_EVENT;

impl X11Input {
    fn connect(display: Option<&str>) -> Result<Self, DesktopError> {
        let display = display.ok_or_else(|| unavailable("no X display configured"))?;
        let (conn, screen) = x11rb::connect(Some(display)).map_err(unavailable)?;
        let root = conn.setup().roots[screen].root;
        conn.xtest_get_version(2, 2)
            .map_err(unavailable)?
            .reply()
            .map_err(|e| unavailable(format!("XTest extension unavailable: {e}")))?;
        let mut input = Self {
            conn,
            root,
            min_keycode: 0,
            per_keycode: 0,
            keymap: Vec::new(),
        };
        input.load_keymap()?;
        Ok(input)
    }

    fn load_keymap(&mut self) -> Result<(), DesktopError> {
        let setup = self.conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let reply = self
            .conn
            .get_keyboard_mapping(min, max - min + 1)
            .map_err(unavailable)?
            .reply()
            .map_err(unavailable)?;
        self.min_keycode = min;
        self.per_keycode = usize::from(reply.keysyms_per_keycode);
        self.keymap = reply.keysyms;
        Ok(())
    }

    /// Keycode producing `keysym`, and whether Shift is needed.
    fn lookup(&self, keysym: u32) -> Option<(u8, bool)> {
        if self.per_keycode == 0 {
            return None;
        }
        self.keymap
            .chunks(self.per_keycode)
            .enumerate()
            .find_map(|(i, syms)| {
                let level = syms.iter().take(2).position(|s| *s == keysym)?;
                Some((self.min_keycode + i as u8, level == 1))
            })
    }

    fn spare_keycode(&self) -> Option<u8> {
        self.keymap
            .chunks(self.per_keycode.max(1))
            .enumerate()
            .rev()
            .find(|(_, syms)| syms.iter().all(|s| *s == 0))
            .map(|(i, _)| self.min_keycode + i as u8)
    }

    fn sync(&self) -> Result<(), DesktopError> {
        self.conn
            .get_input_focus()
            .map_err(unavailable)?
            .reply()
            .map(|_| ())
            .map_err(unavailable)
    }

    fn fake(&self, kind: u8, detail: u8, x: i16, y: i16) -> Result<(), DesktopError> {
        self.conn
            .xtest_fake_input(kind, detail, x11rb::CURRENT_TIME, self.root, x, y, 0)
            .map_err(unavailable)?;
        self.sync()
    }

    fn key_event(&self, keycode: u8, press: bool) -> Result<(), DesktopError> {
        self.fake(if press { KEY_PRESS } else { KEY_RELEASE }, keycode, 0, 0)
    }

    async fn send(&mut self, input: &Input, delay: Duration) -> Result<(), DesktopError> {
        let step = Duration::from_millis(60);
        match input {
            Input::Click { x, y } => {
                let (x, y) = (coord(*x)?, coord(*y)?);
                self.fake(MOTION, 0, x, y)?;
                tokio::time::sleep(step * 2).await;
                self.fake(BUTTON_PRESS, 1, x, y)?;
                tokio::time::sleep(step).await;
                self.fake(BUTTON_RELEASE, 1, x, y)
            }
            Input::Scroll { x, y, notches } => {
                let (x, y) = (coord(*x)?, coord(*y)?);
                self.fake(MOTION, 0, x, y)?;
                let button = if *notches >= 0 { 5 } else { 4 };
                for _ in 0..notches.unsigned_abs() {
                    tokio::time::sleep(step * 2).await;
                    self.fake(BUTTON_PRESS, button, x, y)?;
                    self.fake(BUTTON_RELEASE, button, x, y)?;
                }
                Ok(())
            }
            Input::Key(combo) => {
                let combo = keys::parse_combo(combo)?;
                let mut held = Vec::new();
                for m in &combo.modifiers {
                    let (kc, _) = self
                        .lookup(*m)
                        .ok_or_else(|| DesktopError::UnsupportedKey(format!("{m:#x}")))?;
                    held.push(kc);
                }
                let result = self.press_keysym(combo.key, &held, step).await;
                for kc in held.iter().rev() {
                    self.key_event(*kc, false)?;
                }
                result
            }
            Input::Type(text) => {
                for c in text.chars() {
                    let keysym = keys::char_keysym(c)
                        .ok_or_else(|| DesktopError::UnsupportedKey(c.to_string()))?;
                    self.press_keysym(keysym, &[], Duration::from_millis(15))
                        .await?;
                    tokio::time::sleep(delay).await;
                }
                Ok(())
            }
        }
    }

    /// Presses and releases a keysym, with `held` modifiers already down
    /// (they are pressed here and left for the caller to release).
    async fn press_keysym(
        &mut self,
        keysym: u32,
        held: &[u8],
        step: Duration,
    ) -> Result<(), DesktopError> {
        for kc in held {
            self.key_event(*kc, true)?;
        }
        if let Some((kc, shift)) = self.lookup(keysym) {
            return self.tap(kc, shift, step).await;
        }
        // Not on the keyboard (é, €...): bind it to a spare keycode for the
        // duration of the key press, as xdotool does.
        let kc = self
            .spare_keycode()
            .ok_or_else(|| DesktopError::UnsupportedKey(format!("{keysym:#x}")))?;
        let mut syms = vec![0u32; self.per_keycode];
        syms[0] = keysym;
        if self.per_keycode > 1 {
            syms[1] = keysym;
        }
        self.remap(kc, &syms)?;
        tokio::time::sleep(Duration::from_millis(40)).await;
        let result = self.tap(kc, false, step).await;
        tokio::time::sleep(Duration::from_millis(40)).await;
        self.remap(kc, &vec![0u32; self.per_keycode])?;
        result
    }

    fn remap(&mut self, keycode: u8, syms: &[u32]) -> Result<(), DesktopError> {
        self.conn
            .change_keyboard_mapping(1, keycode, self.per_keycode as u8, syms)
            .map_err(unavailable)?;
        self.sync()?;
        let offset = usize::from(keycode - self.min_keycode) * self.per_keycode;
        self.keymap[offset..offset + self.per_keycode].copy_from_slice(syms);
        Ok(())
    }

    async fn tap(&self, keycode: u8, shift: bool, step: Duration) -> Result<(), DesktopError> {
        let shift_kc = if shift {
            Some(
                self.lookup(SHIFT_L)
                    .map(|(kc, _)| kc)
                    .ok_or_else(|| DesktopError::UnsupportedKey("Shift".into()))?,
            )
        } else {
            None
        };
        if let Some(s) = shift_kc {
            self.key_event(s, true)?;
        }
        self.key_event(keycode, true)?;
        tokio::time::sleep(step).await;
        self.key_event(keycode, false)?;
        if let Some(s) = shift_kc {
            self.key_event(s, false)?;
        }
        Ok(())
    }
}

fn coord(v: i32) -> Result<i16, DesktopError> {
    i16::try_from(v).map_err(|_| DesktopError::NotClickable(format!("coordinate {v}")))
}
