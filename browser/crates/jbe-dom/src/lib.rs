//! Minimal DOM representation and arena storage for Jinn Browser Engine.
//!
//! Provides deterministic node storage, parent-child relations, and safe traversals
//! bounded by `EngineLimits`.

use jbe_core::{EngineError, EngineLimits, NodeId};

/// Type of DOM node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeType {
    /// Root document node.
    Document,
    /// HTML element (e.g., "div", "p", "a").
    Element {
        tag_name: String,
        attributes: Vec<(String, String)>,
    },
    /// Text content inside an element.
    Text(String),
    /// HTML comment.
    Comment(String),
}

/// A node in the DOM tree.
#[derive(Debug, Clone)]
pub struct Node {
    pub id: NodeId,
    pub node_type: NodeType,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
}

impl Node {
    /// Creates a new unlinked node.
    pub fn new(id: NodeId, node_type: NodeType) -> Self {
        Self {
            id,
            node_type,
            parent: None,
            children: Vec::new(),
        }
    }

    /// Returns the element tag name if this is an element.
    pub fn tag_name(&self) -> Option<&str> {
        match &self.node_type {
            NodeType::Element { tag_name, .. } => Some(tag_name),
            _ => None,
        }
    }

    /// Returns attributes if this is an element.
    pub fn attributes(&self) -> Option<&[(String, String)]> {
        match &self.node_type {
            NodeType::Element { attributes, .. } => Some(attributes.as_slice()),
            _ => None,
        }
    }

    /// Looks up an attribute value by name (case-insensitive).
    pub fn get_attribute(&self, name: &str) -> Option<&str> {
        match &self.node_type {
            NodeType::Element { attributes, .. } => {
                for (k, v) in attributes {
                    if k.eq_ignore_ascii_case(name) {
                        return Some(v.as_str());
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Returns the `id` attribute if present.
    pub fn id_attr(&self) -> Option<&str> {
        self.get_attribute("id")
    }

    /// Returns the `class` attribute if present.
    pub fn class_attr(&self) -> Option<&str> {
        self.get_attribute("class")
    }

    /// Returns a list of classes split by whitespace.
    pub fn classes(&self) -> Vec<&str> {
        self.class_attr()
            .map(|c| c.split_whitespace().collect())
            .unwrap_or_default()
    }

    /// Checks if the element contains a given class.
    pub fn has_class(&self, class_name: &str) -> bool {
        self.classes().into_iter().any(|c| c == class_name)
    }

    /// Returns text slice if this is a Text node.
    pub fn as_text(&self) -> Option<&str> {
        match &self.node_type {
            NodeType::Text(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

/// Arena-based DOM Tree.
#[derive(Debug, Clone)]
pub struct DomTree {
    nodes: Vec<Node>,
    root: NodeId,
    limits: EngineLimits,
}

impl DomTree {
    /// Creates a new DOM tree with a root Document node.
    pub fn new(limits: EngineLimits) -> Self {
        let root_id = NodeId::new(0);
        let root_node = Node::new(root_id, NodeType::Document);
        Self {
            nodes: vec![root_node],
            root: root_id,
            limits,
        }
    }

    /// Returns the root Document node ID.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Total number of nodes in the arena.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Borrows a node by its ID.
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.index())
    }

    /// Mutably borrows a node by its ID.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id.index())
    }

    /// Allocates an element node in the arena.
    pub fn create_element(
        &mut self,
        tag: &str,
        attributes: Vec<(String, String)>,
    ) -> Result<NodeId, EngineError> {
        self.check_node_limit()?;
        let id = NodeId::new(self.nodes.len());
        let node = Node::new(
            id,
            NodeType::Element {
                tag_name: tag.to_ascii_lowercase(),
                attributes,
            },
        );
        self.nodes.push(node);
        Ok(id)
    }

    /// Allocates a text node in the arena.
    pub fn create_text(&mut self, text: &str) -> Result<NodeId, EngineError> {
        self.check_node_limit()?;
        let id = NodeId::new(self.nodes.len());
        let node = Node::new(id, NodeType::Text(text.to_string()));
        self.nodes.push(node);
        Ok(id)
    }

    /// Allocates a comment node in the arena.
    pub fn create_comment(&mut self, comment: &str) -> Result<NodeId, EngineError> {
        self.check_node_limit()?;
        let id = NodeId::new(self.nodes.len());
        let node = Node::new(id, NodeType::Comment(comment.to_string()));
        self.nodes.push(node);
        Ok(id)
    }

    /// Appends `child` to `parent`'s children list, setting child's parent pointer.
    pub fn append_child(&mut self, parent: NodeId, child: NodeId) -> Result<(), EngineError> {
        if parent.index() >= self.nodes.len() || child.index() >= self.nodes.len() {
            return Err(EngineError::InvalidNodeId(parent.index()));
        }

        // Prevent self-parenting
        if parent == child {
            return Err(EngineError::Internal("Cannot append node to itself".into()));
        }

        // Check depth limit before linking
        let parent_depth = self.depth_of(parent);
        if parent_depth + 1 > self.limits.max_dom_depth {
            return Err(EngineError::DomLimitExceeded(format!(
                "DOM tree depth exceeded limit {}",
                self.limits.max_dom_depth
            )));
        }

        // Update child's parent
        self.nodes[child.index()].parent = Some(parent);

        // Update parent's children
        if !self.nodes[parent.index()].children.contains(&child) {
            self.nodes[parent.index()].children.push(child);
        }

        Ok(())
    }

    /// Calculates depth from root (root is depth 0).
    pub fn depth_of(&self, mut node_id: NodeId) -> usize {
        let mut depth = 0;
        let mut visited = 0;
        while let Some(node) = self.get(node_id) {
            if let Some(parent) = node.parent {
                depth += 1;
                node_id = parent;
                visited += 1;
                if visited > self.nodes.len() {
                    // Cycle guard
                    break;
                }
            } else {
                break;
            }
        }
        depth
    }

    /// Recursively collects all text under the given node.
    pub fn text_content(&self, id: NodeId) -> String {
        let mut out = String::new();
        self.collect_text(id, &mut out);
        out
    }

    fn collect_text(&self, id: NodeId, out: &mut String) {
        if let Some(node) = self.get(id) {
            match &node.node_type {
                NodeType::Text(t) => {
                    out.push_str(t);
                }
                NodeType::Element { .. } | NodeType::Document => {
                    for &child in &node.children {
                        self.collect_text(child, out);
                    }
                }
                NodeType::Comment(_) => {}
            }
        }
    }

    /// Finds the first element with the given tag name (pre-order traversal).
    pub fn find_first_by_tag(&self, tag: &str) -> Option<NodeId> {
        let tag_lower = tag.to_ascii_lowercase();
        for node in &self.nodes {
            if let Some(name) = node.tag_name() {
                if name == tag_lower {
                    return Some(node.id);
                }
            }
        }
        None
    }

    /// Finds all elements matching the given tag name.
    pub fn find_all_by_tag(&self, tag: &str) -> Vec<NodeId> {
        let tag_lower = tag.to_ascii_lowercase();
        let mut result = Vec::new();
        for node in &self.nodes {
            if let Some(name) = node.tag_name() {
                if name == tag_lower {
                    result.push(node.id);
                }
            }
        }
        result
    }

    /// Helper to enforce max DOM nodes limit.
    fn check_node_limit(&self) -> Result<(), EngineError> {
        if self.nodes.len() >= self.limits.max_dom_nodes {
            Err(EngineError::DomLimitExceeded(format!(
                "DOM node count reached maximum limit of {}",
                self.limits.max_dom_nodes
            )))
        } else {
            Ok(())
        }
    }

    /// Preorder traversal returning all NodeIds starting from root.
    pub fn preorder(&self) -> Vec<NodeId> {
        let mut res = Vec::new();
        self.preorder_helper(self.root, &mut res);
        res
    }

    fn preorder_helper(&self, current: NodeId, res: &mut Vec<NodeId>) {
        res.push(current);
        if let Some(node) = self.get(current) {
            for &child in &node.children {
                self.preorder_helper(child, res);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dom_creation_and_attributes() {
        let mut dom = DomTree::new(EngineLimits::default());
        let div = dom
            .create_element(
                "div",
                vec![
                    ("id".into(), "main".into()),
                    ("class".into(), "container active".into()),
                ],
            )
            .unwrap();

        dom.append_child(dom.root(), div).unwrap();

        let div_node = dom.get(div).unwrap();
        assert_eq!(div_node.tag_name(), Some("div"));
        assert_eq!(div_node.id_attr(), Some("main"));
        assert_eq!(div_node.class_attr(), Some("container active"));
        assert!(div_node.has_class("container"));
        assert!(div_node.has_class("active"));
        assert!(!div_node.has_class("other"));
    }

    #[test]
    fn test_text_and_hierarchy() {
        let mut dom = DomTree::new(EngineLimits::default());
        let p = dom.create_element("p", vec![]).unwrap();
        let txt1 = dom.create_text("Hello ").unwrap();
        let strong = dom.create_element("strong", vec![]).unwrap();
        let txt2 = dom.create_text("World").unwrap();

        dom.append_child(dom.root(), p).unwrap();
        dom.append_child(p, txt1).unwrap();
        dom.append_child(p, strong).unwrap();
        dom.append_child(strong, txt2).unwrap();

        assert_eq!(dom.text_content(p), "Hello World");
        assert_eq!(dom.depth_of(dom.root()), 0);
        assert_eq!(dom.depth_of(p), 1);
        assert_eq!(dom.depth_of(strong), 2);
        assert_eq!(dom.depth_of(txt2), 3);
    }

    #[test]
    fn test_node_limit_enforced() {
        let limits = EngineLimits {
            max_dom_nodes: 5,
            ..Default::default()
        };
        let mut dom = DomTree::new(limits); // Already 1 node (root)
        for _ in 1..5 {
            dom.create_element("span", vec![]).unwrap();
        }
        assert_eq!(dom.node_count(), 5);
        let err = dom.create_element("span", vec![]);
        assert!(matches!(err, Err(EngineError::DomLimitExceeded(_))));
    }

    #[test]
    fn test_depth_limit_enforced() {
        let limits = EngineLimits {
            max_dom_depth: 3,
            ..Default::default()
        };
        let mut dom = DomTree::new(limits);
        let n1 = dom.create_element("div", vec![]).unwrap();
        let n2 = dom.create_element("div", vec![]).unwrap();
        let n3 = dom.create_element("div", vec![]).unwrap();
        let n4 = dom.create_element("div", vec![]).unwrap();

        dom.append_child(dom.root(), n1).unwrap(); // depth 1
        dom.append_child(n1, n2).unwrap(); // depth 2
        dom.append_child(n2, n3).unwrap(); // depth 3
        let res = dom.append_child(n3, n4); // depth 4 -> error!
        assert!(matches!(res, Err(EngineError::DomLimitExceeded(_))));
    }
}
