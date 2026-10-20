//! Owned accessibility tree snapshots.

use serde::{Deserialize, Serialize};

/// Screen rectangle in pixels. Serialized as `[x, y, width, height]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "[i32; 4]", into = "[i32; 4]")]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn center(&self) -> (i32, i32) {
        (self.x + self.width / 2, self.y + self.height / 2)
    }

    pub fn is_empty(&self) -> bool {
        self.width <= 0 || self.height <= 0
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    /// Whether `other` lies entirely inside this rectangle.
    pub fn encloses(&self, other: &Rect) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.x + other.width <= self.x + self.width
            && other.y + other.height <= self.y + self.height
    }
}

impl From<[i32; 4]> for Rect {
    fn from([x, y, width, height]: [i32; 4]) -> Self {
        Self::new(x, y, width, height)
    }
}

impl From<Rect> for [i32; 4] {
    fn from(r: Rect) -> Self {
        [r.x, r.y, r.width, r.height]
    }
}

/// States kept in snapshots (the rest is noise for automation).
pub const KEPT_STATES: &[&str] = &[
    "busy", "checked", "editable", "expanded", "focused", "pressed", "selected", "showing",
];

/// One accessible object and its subtree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    /// AT-SPI role name, e.g. `"button"`, `"combo box"`, `"document web"`.
    pub role: String,
    /// Accessible name (label).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Text content of editable fields and combo boxes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Bounds in screen coordinates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Rect>,
    /// Subset of states, see [`KEPT_STATES`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Node>,
}

impl Node {
    pub fn new(role: &str, name: &str) -> Self {
        Self {
            role: role.to_owned(),
            name: name.to_owned(),
            ..Self::default()
        }
    }

    pub fn with_bounds(mut self, bounds: Rect) -> Self {
        self.bounds = Some(bounds);
        self
    }

    pub fn with_children(mut self, children: Vec<Node>) -> Self {
        self.children = children;
        self
    }

    pub fn has_state(&self, state: &str) -> bool {
        self.states.iter().any(|s| s == state)
    }

    /// Depth-first, pre-order iterator over this node and its descendants.
    pub fn iter(&self) -> impl Iterator<Item = &Node> {
        let mut stack = vec![self];
        std::iter::from_fn(move || {
            let node = stack.pop()?;
            stack.extend(node.children.iter().rev());
            Some(node)
        })
    }

    /// First node (pre-order) matching `pred`.
    pub fn find(&self, pred: impl Fn(&Node) -> bool) -> Option<&Node> {
        self.iter().find(|n| pred(n))
    }

    /// All nodes (pre-order) matching `pred`.
    pub fn find_all(&self, pred: impl Fn(&Node) -> bool) -> Vec<&Node> {
        self.iter().filter(|n| pred(n)).collect()
    }

    /// First node with this role whose name satisfies `pred`.
    pub fn find_role(&self, role: &str, pred: impl Fn(&str) -> bool) -> Option<&Node> {
        self.find(|n| n.role == role && pred(&n.name))
    }

    /// Non-empty names of this subtree, pre-order: the visible text, roughly
    /// in reading order.
    pub fn texts(&self) -> Vec<&str> {
        self.iter()
            .map(|n| n.name.trim())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Whether any name or value in the subtree contains `needle`
    /// (case-insensitive).
    pub fn contains_text(&self, needle: &str) -> bool {
        let needle = needle.to_lowercase();
        self.iter().any(|n| {
            n.name.to_lowercase().contains(&needle)
                || n.value
                    .as_deref()
                    .is_some_and(|v| v.to_lowercase().contains(&needle))
        })
    }

    /// Centre of the node, if it has a non-empty on-screen area.
    pub fn center(&self) -> Option<(i32, i32)> {
        self.bounds.filter(|b| !b.is_empty()).map(|b| b.center())
    }

    /// Number of nodes in the subtree.
    pub fn node_count(&self) -> usize {
        self.iter().count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Node {
        Node::new("frame", "Window").with_children(vec![
            Node::new("button", "OK").with_bounds(Rect::new(10, 10, 20, 10)),
            Node::new("section", "").with_children(vec![Node::new("static", "Hello")]),
        ])
    }

    #[test]
    fn traversal_is_preorder() {
        let roles: Vec<_> = sample().iter().map(|n| n.role.clone()).collect();
        assert_eq!(roles, ["frame", "button", "section", "static"]);
        assert_eq!(sample().texts(), ["Window", "OK", "Hello"]);
    }

    #[test]
    fn find_and_center() {
        let tree = sample();
        let ok = tree.find_role("button", |n| n == "OK").unwrap();
        assert_eq!(ok.center(), Some((20, 15)));
        assert!(tree.contains_text("hello"));
    }

    #[test]
    fn compact_json() {
        let json =
            serde_json::to_string(&Node::new("button", "OK").with_bounds(Rect::new(1, 2, 3, 4)))
                .unwrap();
        assert_eq!(json, r#"{"role":"button","name":"OK","bounds":[1,2,3,4]}"#);
        let back: Node = serde_json::from_str(&json).unwrap();
        assert_eq!(back.bounds, Some(Rect::new(1, 2, 3, 4)));
    }
}
