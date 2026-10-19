//! The app screen as a tree of nodes, from `uiautomator dump`.

use std::fmt;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

/// A pixel position on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// A node's rectangle on screen, in pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Bounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Bounds {
    /// Parses uiautomator's `[left,top][right,bottom]`.
    pub fn parse(s: &str) -> Option<Self> {
        let nums: Vec<i32> = s
            .split(|c: char| !(c.is_ascii_digit() || c == '-'))
            .filter(|p| !p.is_empty())
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()?;
        match nums[..] {
            [left, top, right, bottom] => Some(Self {
                left,
                top,
                right,
                bottom,
            }),
            _ => None,
        }
    }

    pub fn center(&self) -> Point {
        Point {
            x: (self.left + self.right) / 2,
            y: (self.top + self.bottom) / 2,
        }
    }

    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }

    pub fn is_empty(&self) -> bool {
        self.width() <= 0 || self.height() <= 0
    }
}

/// One view on screen. Empty strings mean "absent".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UiNode {
    pub class: String,
    pub package: String,
    pub resource_id: String,
    pub text: String,
    pub content_desc: String,
    pub bounds: Bounds,
    pub clickable: bool,
    pub enabled: bool,
    pub focused: bool,
    pub scrollable: bool,
    pub selected: bool,
    pub checked: bool,
    pub children: Vec<UiNode>,
}

impl UiNode {
    /// This node and all its descendants, depth-first, in screen order.
    pub fn iter(&self) -> impl Iterator<Item = &UiNode> {
        let mut stack = vec![self];
        std::iter::from_fn(move || {
            let node = stack.pop()?;
            stack.extend(node.children.iter().rev());
            Some(node)
        })
    }

    /// All visible text of this subtree (text, else content description), in order.
    pub fn texts(&self) -> Vec<&str> {
        self.iter()
            .filter_map(|n| {
                [n.text.as_str(), n.content_desc.as_str()]
                    .into_iter()
                    .find(|s| !s.trim().is_empty())
            })
            .collect()
    }

    pub fn find(&self, by: &By) -> Option<&UiNode> {
        self.iter().find(|n| by.matches(n))
    }

    pub fn find_all<'a>(&'a self, by: &'a By) -> impl Iterator<Item = &'a UiNode> + 'a {
        self.iter().filter(move |n| by.matches(n))
    }
}

/// A whole screen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UiTree {
    /// Top-level windows (usually one).
    pub roots: Vec<UiNode>,
}

impl UiTree {
    /// Parses a uiautomator XML dump. Extra text around the XML is ignored.
    pub fn parse(xml: &str) -> Result<Self, String> {
        let start = xml.find("<hierarchy").ok_or("no <hierarchy> in dump")?;
        let end = xml
            .rfind("</hierarchy>")
            .map(|i| i + "</hierarchy>".len())
            .ok_or("truncated dump")?;
        let mut reader = Reader::from_str(&xml[start..end]);
        let mut stack: Vec<UiNode> = Vec::new();
        let mut roots = Vec::new();
        loop {
            match reader.read_event().map_err(|e| e.to_string())? {
                Event::Start(e) if e.name().as_ref() == b"node" => stack.push(node(&e)?),
                Event::Empty(e) if e.name().as_ref() == b"node" => {
                    attach(&mut stack, &mut roots, node(&e)?);
                }
                Event::End(e) if e.name().as_ref() == b"node" => {
                    let done = stack.pop().ok_or("unbalanced </node>")?;
                    attach(&mut stack, &mut roots, done);
                }
                Event::Eof => break,
                _ => {}
            }
        }
        if !stack.is_empty() {
            return Err("unclosed <node>".into());
        }
        Ok(Self { roots })
    }

    pub fn iter(&self) -> impl Iterator<Item = &UiNode> {
        self.roots.iter().flat_map(UiNode::iter)
    }

    /// Package of the app in the foreground (first window).
    pub fn package(&self) -> Option<&str> {
        self.roots
            .first()
            .map(|r| r.package.as_str())
            .filter(|p| !p.is_empty())
    }

    /// Full screen size, from the first window.
    pub fn screen(&self) -> Option<Bounds> {
        self.roots.first().map(|r| r.bounds)
    }

    pub fn find(&self, by: &By) -> Option<&UiNode> {
        self.iter().find(|n| by.matches(n))
    }

    pub fn find_all<'a>(&'a self, by: &'a By) -> impl Iterator<Item = &'a UiNode> + 'a {
        self.iter().filter(move |n| by.matches(n))
    }

    pub fn contains(&self, by: &By) -> bool {
        self.find(by).is_some()
    }
}

/// Compact one-line-per-node outline, for debugging connectors.
impl fmt::Display for UiTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn walk(n: &UiNode, depth: usize, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let class = n.class.rsplit('.').next().unwrap_or_default();
            write!(f, "{:indent$}{class}", "", indent = depth * 2)?;
            if !n.resource_id.is_empty() {
                write!(f, " #{}", n.resource_id)?;
            }
            if !n.text.is_empty() {
                write!(f, " {:?}", n.text)?;
            }
            if !n.content_desc.is_empty() {
                write!(f, " desc={:?}", n.content_desc)?;
            }
            if n.clickable {
                f.write_str(" [click]")?;
            }
            if n.scrollable {
                f.write_str(" [scroll]")?;
            }
            writeln!(f)?;
            n.children.iter().try_for_each(|c| walk(c, depth + 1, f))
        }
        self.roots.iter().try_for_each(|r| walk(r, 0, f))
    }
}

fn attach(stack: &mut [UiNode], roots: &mut Vec<UiNode>, node: UiNode) {
    match stack.last_mut() {
        Some(parent) => parent.children.push(node),
        None => roots.push(node),
    }
}

fn node(e: &BytesStart<'_>) -> Result<UiNode, String> {
    let mut n = UiNode::default();
    for attr in e.attributes() {
        let attr = attr.map_err(|e| e.to_string())?;
        let value = attr
            .unescape_value()
            .map_err(|e| e.to_string())?
            .into_owned();
        let flag = value == "true";
        match attr.key.as_ref() {
            b"class" => n.class = value,
            b"package" => n.package = value,
            b"resource-id" => n.resource_id = value,
            b"text" => n.text = value,
            b"content-desc" => n.content_desc = value,
            b"bounds" => n.bounds = Bounds::parse(&value).unwrap_or_default(),
            b"clickable" => n.clickable = flag,
            b"enabled" => n.enabled = flag,
            b"focused" => n.focused = flag,
            b"scrollable" => n.scrollable = flag,
            b"selected" => n.selected = flag,
            b"checked" => n.checked = flag,
            _ => {}
        }
    }
    Ok(n)
}

/// A node selector. All set criteria must match.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct By {
    resource_id: Option<String>,
    text: Option<String>,
    text_contains: Option<String>,
    content_desc: Option<String>,
    class: Option<String>,
    clickable: Option<bool>,
}

impl By {
    /// Full resource id (`com.example:id/name`).
    pub fn id(id: impl Into<String>) -> Self {
        Self {
            resource_id: Some(id.into()),
            ..Self::default()
        }
    }

    /// Exact text.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            ..Self::default()
        }
    }

    /// Text containing a substring, case-insensitive.
    pub fn text_contains(text: impl Into<String>) -> Self {
        Self {
            text_contains: Some(text.into().to_lowercase()),
            ..Self::default()
        }
    }

    /// Exact content description (accessibility label).
    pub fn desc(desc: impl Into<String>) -> Self {
        Self {
            content_desc: Some(desc.into()),
            ..Self::default()
        }
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.class = Some(class.into());
        self
    }

    pub fn clickable(mut self, clickable: bool) -> Self {
        self.clickable = Some(clickable);
        self
    }

    pub fn matches(&self, n: &UiNode) -> bool {
        let eq = |want: &Option<String>, have: &str| want.as_deref().is_none_or(|w| w == have);
        eq(&self.resource_id, &n.resource_id)
            && eq(&self.text, &n.text)
            && eq(&self.content_desc, &n.content_desc)
            && eq(&self.class, &n.class)
            && self.clickable.is_none_or(|c| c == n.clickable)
            && self
                .text_contains
                .as_deref()
                .is_none_or(|t| n.text.to_lowercase().contains(t))
    }
}

impl fmt::Display for By {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if let Some(v) = &self.resource_id {
            parts.push(format!("id={v}"));
        }
        if let Some(v) = &self.text {
            parts.push(format!("text={v:?}"));
        }
        if let Some(v) = &self.text_contains {
            parts.push(format!("text~{v:?}"));
        }
        if let Some(v) = &self.content_desc {
            parts.push(format!("desc={v:?}"));
        }
        if let Some(v) = &self.class {
            parts.push(format!("class={v}"));
        }
        if let Some(v) = self.clickable {
            parts.push(format!("clickable={v}"));
        }
        write!(f, "[{}]", parts.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAUNCHER: &str = include_str!("../tests/fixtures/launcher.xml");

    #[test]
    fn parses_a_dump() {
        let tree = UiTree::parse(LAUNCHER).unwrap();
        assert_eq!(tree.package(), Some("com.android.launcher3"));
        assert_eq!(
            tree.screen(),
            Some(Bounds {
                left: 0,
                top: 0,
                right: 720,
                bottom: 1440
            })
        );
        let gallery = tree.find(&By::desc("Gallery")).unwrap();
        assert!(gallery.clickable);
        assert_eq!(gallery.bounds.center(), Point { x: 274, y: 1032 });
        assert_eq!(tree.find_all(&By::default().clickable(true)).count(), 6);
        assert!(tree.contains(&By::text_contains("galler")));
        assert!(!tree.contains(&By::text("Nope")));
    }

    #[test]
    fn ignores_noise_around_the_xml() {
        let noisy = format!("garbage{LAUNCHER}\nUI hierchary dumped to: /dev/tty\n");
        assert_eq!(
            UiTree::parse(&noisy).unwrap(),
            UiTree::parse(LAUNCHER).unwrap()
        );
        assert!(UiTree::parse("nothing").is_err());
    }

    #[test]
    fn bounds() {
        let b = Bounds::parse("[10,20][110,220]").unwrap();
        assert_eq!((b.width(), b.height()), (100, 200));
        assert_eq!(b.center(), Point { x: 60, y: 120 });
        assert!(Bounds::parse("[1,2]").is_none());
    }

    #[test]
    fn outline() {
        let out = UiTree::parse(LAUNCHER).unwrap().to_string();
        assert!(
            out.contains("TextView \"Gallery\" desc=\"Gallery\" [click]"),
            "{out}"
        );
    }
}
