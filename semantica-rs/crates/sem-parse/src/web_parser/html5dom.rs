//! A minimal reference-counted DOM for html5ever's HTML5 tree builder, used by the static
//! WebDriver: it parses a page the way a browser does (implied `html`/`head`/`body`,
//! foster parenting, ...) and serialises it with html5ever's HTML serialiser, which is what a
//! browser's `page_source` returns for a page whose scripts do not change the DOM.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::io;
use std::rc::{Rc, Weak};

use html5ever::serialize::{Serialize, SerializeOpts, Serializer, TraversalScope};
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::{Attribute, ParseOpts, QualName, parse_document};

/// A DOM node handle.
pub(crate) type Handle = Rc<Node>;

/// The payload of a DOM node.
pub(crate) enum NodeData {
    Document,
    Doctype {
        name: String,
    },
    Text {
        contents: RefCell<String>,
    },
    Comment {
        contents: String,
    },
    Element {
        name: QualName,
        attrs: RefCell<Vec<Attribute>>,
        template_contents: Option<Handle>,
    },
    ProcessingInstruction {
        target: String,
        contents: String,
    },
}

/// A DOM node.
pub(crate) struct Node {
    pub(crate) data: NodeData,
    parent: Cell<Option<Weak<Node>>>,
    pub(crate) children: RefCell<Vec<Handle>>,
}

impl Node {
    fn new(data: NodeData) -> Handle {
        Rc::new(Node {
            data,
            parent: Cell::new(None),
            children: RefCell::new(Vec::new()),
        })
    }

    /// The element's local name (`None` for other nodes).
    pub(crate) fn local_name(&self) -> Option<&str> {
        match &self.data {
            NodeData::Element { name, .. } => Some(&name.local),
            _ => None,
        }
    }

    /// An attribute value of an element.
    pub(crate) fn attr(&self, key: &str) -> Option<String> {
        match &self.data {
            NodeData::Element { attrs, .. } => attrs
                .borrow()
                .iter()
                .find(|a| &*a.name.local == key)
                .map(|a| a.value.to_string()),
            _ => None,
        }
    }

    fn parent(&self) -> Option<Handle> {
        let weak = self.parent.take();
        let strong = weak.as_ref().and_then(Weak::upgrade);
        self.parent.set(weak);
        strong
    }
}

fn append(parent: &Handle, child: Handle) {
    let previous = child.parent.replace(Some(Rc::downgrade(parent)));
    assert!(previous.is_none(), "child already has a parent");
    parent.children.borrow_mut().push(child);
}

fn get_parent_and_index(target: &Handle) -> Option<(Handle, usize)> {
    let parent = target.parent()?;
    let i = parent
        .children
        .borrow()
        .iter()
        .position(|child| Rc::ptr_eq(child, target))
        .expect("child not in parent");
    Some((parent, i))
}

fn append_to_existing_text(prev: &Handle, text: &str) -> bool {
    match &prev.data {
        NodeData::Text { contents } => {
            contents.borrow_mut().push_str(text);
            true
        }
        _ => false,
    }
}

fn remove_from_parent(target: &Handle) {
    if let Some((parent, i)) = get_parent_and_index(target) {
        parent.children.borrow_mut().remove(i);
        target.parent.set(None);
    }
}

/// The tree sink html5ever builds into.
pub(crate) struct Dom {
    pub(crate) document: Handle,
    quirks_mode: Cell<QuirksMode>,
}

impl Default for Dom {
    fn default() -> Self {
        Dom {
            document: Node::new(NodeData::Document),
            quirks_mode: Cell::new(QuirksMode::NoQuirks),
        }
    }
}

impl TreeSink for Dom {
    type Handle = Handle;
    type Output = Self;
    type ElemName<'a> = &'a QualName;

    fn finish(self) -> Self {
        self
    }

    fn parse_error(&self, _msg: Cow<'static, str>) {}

    fn get_document(&self) -> Handle {
        self.document.clone()
    }

    fn elem_name<'a>(&'a self, target: &'a Handle) -> &'a QualName {
        match &target.data {
            NodeData::Element { name, .. } => name,
            _ => panic!("not an element"),
        }
    }

    fn create_element(&self, name: QualName, attrs: Vec<Attribute>, flags: ElementFlags) -> Handle {
        Node::new(NodeData::Element {
            name,
            attrs: RefCell::new(attrs),
            template_contents: if flags.template {
                Some(Node::new(NodeData::Document))
            } else {
                None
            },
        })
    }

    fn create_comment(&self, text: StrTendril) -> Handle {
        Node::new(NodeData::Comment {
            contents: text.to_string(),
        })
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> Handle {
        Node::new(NodeData::ProcessingInstruction {
            target: target.to_string(),
            contents: data.to_string(),
        })
    }

    fn append(&self, parent: &Handle, child: NodeOrText<Handle>) {
        match child {
            NodeOrText::AppendText(text) => {
                if let Some(last) = parent.children.borrow().last()
                    && append_to_existing_text(last, &text)
                {
                    return;
                }
                append(
                    parent,
                    Node::new(NodeData::Text {
                        contents: RefCell::new(text.to_string()),
                    }),
                );
            }
            NodeOrText::AppendNode(node) => append(parent, node),
        }
    }

    fn append_based_on_parent_node(&self, element: &Handle, prev_element: &Handle, child: NodeOrText<Handle>) {
        if element.parent().is_some() {
            self.append_before_sibling(element, child);
        } else {
            self.append(prev_element, child);
        }
    }

    fn append_doctype_to_document(&self, name: StrTendril, _public_id: StrTendril, _system_id: StrTendril) {
        append(
            &self.document,
            Node::new(NodeData::Doctype {
                name: name.to_string(),
            }),
        );
    }

    fn get_template_contents(&self, target: &Handle) -> Handle {
        match &target.data {
            NodeData::Element {
                template_contents: Some(contents),
                ..
            } => contents.clone(),
            _ => panic!("not a template element"),
        }
    }

    fn same_node(&self, x: &Handle, y: &Handle) -> bool {
        Rc::ptr_eq(x, y)
    }

    fn set_quirks_mode(&self, mode: QuirksMode) {
        self.quirks_mode.set(mode);
    }

    fn append_before_sibling(&self, sibling: &Handle, child: NodeOrText<Handle>) {
        let (parent, i) = get_parent_and_index(sibling).expect("append_before_sibling called on node without parent");
        let child = match (child, i) {
            (NodeOrText::AppendText(text), 0) => Node::new(NodeData::Text {
                contents: RefCell::new(text.to_string()),
            }),
            (NodeOrText::AppendText(text), i) => {
                let children = parent.children.borrow();
                let prev = &children[i - 1];
                if append_to_existing_text(prev, &text) {
                    return;
                }
                Node::new(NodeData::Text {
                    contents: RefCell::new(text.to_string()),
                })
            }
            (NodeOrText::AppendNode(node), _) => node,
        };
        remove_from_parent(&child);
        child.parent.set(Some(Rc::downgrade(&parent)));
        parent.children.borrow_mut().insert(i, child);
    }

    fn add_attrs_if_missing(&self, target: &Handle, attrs: Vec<Attribute>) {
        if let NodeData::Element { attrs: existing, .. } = &target.data {
            let mut existing = existing.borrow_mut();
            for attr in attrs {
                if !existing.iter().any(|e| e.name == attr.name) {
                    existing.push(attr);
                }
            }
        }
    }

    fn remove_from_parent(&self, target: &Handle) {
        remove_from_parent(target);
    }

    fn reparent_children(&self, node: &Handle, new_parent: &Handle) {
        let mut children = node.children.borrow_mut();
        let mut new_children = new_parent.children.borrow_mut();
        for child in children.iter() {
            child.parent.set(Some(Rc::downgrade(new_parent)));
        }
        new_children.extend(std::mem::take(&mut *children));
    }
}

/// Serialisation of a node and its subtree (html5ever's `Serialize` protocol).
pub(crate) struct SerializableHandle(pub(crate) Handle);

impl Serialize for SerializableHandle {
    fn serialize<S: Serializer>(&self, serializer: &mut S, traversal_scope: TraversalScope) -> io::Result<()> {
        enum Op {
            Open(Handle),
            Close(QualName),
        }
        let mut ops: Vec<Op> = match traversal_scope {
            TraversalScope::IncludeNode => vec![Op::Open(self.0.clone())],
            TraversalScope::ChildrenOnly(_) => self.0.children.borrow().iter().rev().map(|c| Op::Open(c.clone())).collect(),
        };
        while let Some(op) = ops.pop() {
            match op {
                Op::Open(handle) => match &handle.data {
                    NodeData::Element { name, attrs, template_contents } => {
                        serializer.start_elem(name.clone(), attrs.borrow().iter().map(|a| (&a.name, &a.value[..])))?;
                        ops.push(Op::Close(name.clone()));
                        let children = match template_contents {
                            Some(contents) => contents.children.borrow().clone(),
                            None => handle.children.borrow().clone(),
                        };
                        for child in children.iter().rev() {
                            ops.push(Op::Open(child.clone()));
                        }
                    }
                    NodeData::Doctype { name } => serializer.write_doctype(name)?,
                    NodeData::Text { contents } => serializer.write_text(&contents.borrow())?,
                    NodeData::Comment { contents } => serializer.write_comment(contents)?,
                    NodeData::ProcessingInstruction { target, contents } => {
                        serializer.write_processing_instruction(target, contents)?
                    }
                    NodeData::Document => {
                        for child in handle.children.borrow().iter().rev() {
                            ops.push(Op::Open(child.clone()));
                        }
                    }
                },
                Op::Close(name) => serializer.end_elem(name)?,
            }
        }
        Ok(())
    }
}

/// Parse an HTML document with the HTML5 tree-construction algorithm.
pub(crate) fn parse_html(text: &str) -> Dom {
    parse_document(Dom::default(), ParseOpts::default()).one(text)
}

/// The document element (`document.documentElement`).
pub(crate) fn document_element(dom: &Dom) -> Option<Handle> {
    dom.document
        .children
        .borrow()
        .iter()
        .find(|c| matches!(c.data, NodeData::Element { .. }))
        .cloned()
}

/// Serialise a node including itself (`outerHTML`).
pub(crate) fn outer_html(node: &Handle) -> String {
    let mut out: Vec<u8> = Vec::new();
    let opts = SerializeOpts {
        traversal_scope: TraversalScope::IncludeNode,
        ..SerializeOpts::default()
    };
    html5ever::serialize(&mut out, &SerializableHandle(node.clone()), opts).expect("writing to a Vec cannot fail");
    String::from_utf8(out).unwrap_or_default()
}

/// Every element of the document, in document order (template contents excluded).
pub(crate) fn elements(dom: &Dom) -> Vec<Handle> {
    let mut out = Vec::new();
    let mut stack: Vec<Handle> = dom.document.children.borrow().iter().rev().cloned().collect();
    while let Some(node) = stack.pop() {
        if matches!(node.data, NodeData::Element { .. }) {
            out.push(node.clone());
        }
        // Template contents live in a separate fragment that selectors do not reach.
        let children: Vec<Handle> = node.children.borrow().clone();
        for child in children.into_iter().rev() {
            stack.push(child);
        }
    }
    out
}

/// The element's parent element, if any.
pub(crate) fn parent_element(node: &Handle) -> Option<Handle> {
    node.parent().filter(|p| matches!(p.data, NodeData::Element { .. }))
}
