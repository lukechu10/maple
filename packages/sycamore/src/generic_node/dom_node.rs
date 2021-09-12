//! Rendering backend for the DOM.

use std::cell::Cell;
use std::fmt;
use std::hash::{Hash, Hasher};

use wasm_bindgen::prelude::*;
use wasm_bindgen::{intern, JsCast};
use web_sys::{Comment, Element, Node, Text};

use crate::generic_node::{EventHandler, GenericNode};
use crate::reactive::{create_root, on_cleanup, ReactiveScope};
use crate::template::Template;
use crate::utils::render::insert;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = Node)]
    type NodeWithId;

    #[wasm_bindgen(method, getter, js_name = "$$$nodeId")]
    fn node_id(this: &NodeWithId) -> Option<usize>;

    #[wasm_bindgen(method, setter, js_name = "$$$nodeId")]
    fn set_node_id(this: &NodeWithId, id: usize);
}

/// An unique id for every node.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct NodeId(usize);

impl Default for NodeId {
    fn default() -> Self {
        Self(0)
    }
}

impl NodeId {
    fn new_with_node(node: &Node) -> Self {
        thread_local!(static NODE_ID_COUNTER: Cell<usize> = Cell::new(1)); // 0 is reserved for default value.

        let id = NODE_ID_COUNTER.with(|x| {
            let tmp = x.get();
            x.set(tmp + 1);
            tmp
        });
        node.unchecked_ref::<NodeWithId>().set_node_id(id);
        Self(id)
    }
}

/// Rendering backend for the DOM.
///
/// _This API requires the following crate features to be activated: `dom`_
#[derive(Clone)]
pub struct DomNode {
    id: Cell<NodeId>,
    node: Node,
}

impl DomNode {
    /// Create a new [`DomNode`] wrapping a [`Node`].
    #[inline]
    pub fn new(node: Node) -> Self {
        Self {
            id: Default::default(),
            node,
        }
    }

    /// Gets the underlying [`Node`].
    pub fn inner_element(&self) -> Node {
        self.node.clone()
    }

    pub fn unchecked_into<T: JsCast>(self) -> T {
        self.node.unchecked_into()
    }

    fn get_node_id(&self) -> NodeId {
        if self.id.get().0 == 0 {
            // self.id not yet initialized.
            if let Some(id) = self.node.unchecked_ref::<NodeWithId>().node_id() {
                self.id.set(NodeId(id));
            } else {
                self.id.set(NodeId::new_with_node(&self.node));
            }
        }
        self.id.get()
    }
}

impl PartialEq for DomNode {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node
    }
}

impl Eq for DomNode {}

impl Hash for DomNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.get_node_id().hash(state);
    }
}

impl AsRef<JsValue> for DomNode {
    fn as_ref(&self) -> &JsValue {
        self.node.as_ref()
    }
}

impl From<DomNode> for JsValue {
    fn from(node: DomNode) -> Self {
        (*node.node).clone().into()
    }
}

impl fmt::Debug for DomNode {
    /// Prints outerHtml of [`Node`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let outer_html = if let Some(element) = self.node.dyn_ref::<Element>() {
            element.outer_html()
        } else if let Some(text) = self.node.dyn_ref::<Text>() {
            text.text_content().unwrap_or_default()
        } else if let Some(comment) = self.node.dyn_ref::<Comment>() {
            format!("<!--{}-->", comment.text_content().unwrap_or_default())
        } else {
            self.node.to_string().as_string().unwrap_throw()
        };
        f.debug_tuple("DomNode").field(&outer_html).finish()
    }
}

fn document() -> web_sys::Document {
    thread_local! {
        /// Cache document since it is frequently accessed to prevent going through js-interop.
        static DOCUMENT: web_sys::Document = web_sys::window().unwrap_throw().document().unwrap_throw();
    };
    DOCUMENT.with(|document| document.clone())
}

impl GenericNode for DomNode {
    fn element(tag: &str) -> Self {
        let node = document()
            .create_element(intern(tag))
            .unwrap_throw()
            .dyn_into()
            .unwrap();
        Self::new(node)
    }

    fn text_node(text: &str) -> Self {
        let node = document().create_text_node(text).into();
        Self::new(node)
    }

    fn marker() -> Self {
        let node = document().create_comment("").into();
        Self::new(node)
    }

    fn set_attribute(&self, name: &str, value: &str) {
        self.node
            .unchecked_ref::<Element>()
            .set_attribute(intern(name), value)
            .unwrap_throw();
    }

    fn remove_attribute(&self, name: &str) {
        self.node
            .unchecked_ref::<Element>()
            .remove_attribute(intern(name))
            .unwrap_throw();
    }

    fn set_class_name(&self, value: &str) {
        self.node.unchecked_ref::<Element>().set_class_name(value);
    }

    fn set_property(&self, name: &str, value: &JsValue) {
        assert!(js_sys::Reflect::set(&self.node, &name.into(), value).unwrap_throw());
    }

    fn append_child(&self, child: &Self) {
        self.node.append_child(&child.node).unwrap_throw();
    }

    fn first_child(&self) -> Option<Self> {
        self.node.first_child().map(Self::new)
    }

    fn insert_child_before(&self, new_node: &Self, reference_node: Option<&Self>) {
        self.node
            .insert_before(&new_node.node, reference_node.map(|n| n.node.as_ref()))
            .unwrap_throw();
    }

    fn remove_child(&self, child: &Self) {
        self.node.remove_child(&child.node).unwrap_throw();
    }

    fn replace_child(&self, old: &Self, new: &Self) {
        self.node.replace_child(&new.node, &old.node).unwrap_throw();
    }

    fn insert_sibling_before(&self, child: &Self) {
        self.node
            .unchecked_ref::<Element>()
            .before_with_node_1(&child.node)
            .unwrap_throw();
    }

    fn parent_node(&self) -> Option<Self> {
        self.node.parent_node().map(Self::new)
    }

    fn next_sibling(&self) -> Option<Self> {
        self.node.next_sibling().map(Self::new)
    }

    fn remove_self(&self) {
        self.node.unchecked_ref::<Element>().remove();
    }

    fn event(&self, name: &str, handler: Box<EventHandler>) {
        let closure = Closure::wrap(handler);
        self.node
            .add_event_listener_with_callback(intern(name), closure.as_ref().unchecked_ref())
            .unwrap_throw();

        on_cleanup(move || {
            drop(closure);
        });
    }

    fn update_inner_text(&self, text: &str) {
        self.node.set_text_content(Some(text));
    }

    fn dangerously_set_inner_html(&self, html: &str) {
        self.node.unchecked_ref::<Element>().set_inner_html(html);
    }

    fn clone_node(&self) -> Self {
        let node = self.node.clone_node_with_deep(true).unwrap();
        Self::new(node)
    }
}

/// Render a [`Template`] into the DOM.
/// Alias for [`render_to`] with `parent` being the `<body>` tag.
///
/// _This API requires the following crate features to be activated: `dom`_
pub fn render(template: impl FnOnce() -> Template<DomNode>) {
    let window = web_sys::window().unwrap_throw();
    let document = window.document().unwrap_throw();

    render_to(template, &document.body().unwrap_throw());
}

/// Render a [`Template`] under a `parent` node.
/// For rendering under the `<body>` tag, use [`render`] instead.
///
/// _This API requires the following crate features to be activated: `dom`_
pub fn render_to(template: impl FnOnce() -> Template<DomNode>, parent: &Node) {
    let scope = create_root(|| {
        insert(&DomNode::new(parent.clone()), template(), None, None, false);
    });

    thread_local! {
        static GLOBAL_SCOPES: std::cell::RefCell<Vec<ReactiveScope>> = std::cell::RefCell::new(Vec::new());
    }

    GLOBAL_SCOPES.with(|global_scopes| global_scopes.borrow_mut().push(scope));
}
