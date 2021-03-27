use std::cell::RefCell;
use std::fmt::Debug;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, Node};

use crate::prelude::*;

type EventListener = dyn Fn(Event);
pub trait GenericNode: Debug + Clone + PartialEq + Eq + 'static {
    fn element(tag: &str) -> Self;
    fn text_node(text: &str) -> Self;
    fn fragment() -> Self;
    fn marker() -> Self;
    
    fn append_child(&self, child: &Self);
    fn insert_before_self(&self, new_node: &Self);

    #[deprecated]
    fn insert_node_before(&self, newNode: &Self, referenceNode: Option<&Self>);
    fn remove_child(&self, child: &Self);
    fn remove_self(&self);
    fn replace_child(&self, old: &Self, new: &Self);
    fn insert_sibling_before(&self, child: &Self);
    fn parent_node(&self) -> Option<Self>;
    fn next_sibling(&self) -> Option<Self>;
    fn remove_self(&self);
    fn event(&self, name: &str, handler: Box<EventListener>);
    fn update_text(&self, text: &str);
    fn append_render(&self, child: Box<dyn Fn() -> Box<dyn Render<Self>>>) {
        let parent = self.clone();

        let node = create_effect_initial(cloned!((parent) => move || {
            let node = RefCell::new(child().render());

            let effect = cloned!((node) => move || {
                let new_node = child().update_node(&parent, &node.borrow());
                *node.borrow_mut() = new_node;
            });

            (Rc::new(effect), node)
        }));

        parent.append_child(&node.borrow());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomNode {
    node: Node,
}

impl DomNode {
    pub fn inner_element(&self) -> Node {
        self.node.clone()
    }
}

impl GenericNode for DomNode {
    fn element(tag: &str) -> Self {
        DomNode {
            node: web_sys::window()
                .unwrap()
                .document()
                .unwrap()
                .create_element(tag)
                .unwrap()
                .dyn_into()
                .unwrap(),
        }
    }

    fn text_node(text: &str) -> Self {
        DomNode {
            node: web_sys::window()
                .unwrap()
                .document()
                .unwrap()
                .create_text_node(text)
                .into(),
        }
    }

    fn fragment() -> Self {
        DomNode {
            node: web_sys::window()
                .unwrap()
                .document()
                .unwrap()
                .create_document_fragment()
                .into(),
        }
    }

    fn marker() -> Self {
        DomNode {
            node: web_sys::window()
                .unwrap()
                .document()
                .unwrap()
                .create_comment("")
                .into(),
        }
    }

    fn append_child(&self, child: &Self) {
        self.node.append_child(&child.node).unwrap();
    }

    fn insert_before_self(&self, new_node: &Self) {}

    fn insert_node_before(&self, newNode: &Self, referenceNode: Option<&Self>) {
        todo!()
    }

    fn remove_child(&self, child: &Self) {
        self.node.remove_child(&child.node);
    }

    fn remove_self(&self) {
        self.node.unchecked_ref::<HtmlElement>().remove();
    }

    fn replace_child(&self, old: &Self, new: &Self) {
        self.node.replace_child(&old.node, &new.node);
    }

    fn insert_sibling_before(&self, child: &Self) {
        self.node
            .unchecked_ref::<Element>()
            .before_with_node_1(&child.node);
    }

    fn parent_node(&self) -> Option<Self> {
        self.node.parent_node().map(|node| Self { node })
    }

    fn next_sibling(&self) -> Option<Self> {
        self.node.next_sibling().map(|node| Self { node })
    }

    fn remove_self(&self) {
        self.node.unchecked_ref::<Element>().remove();
    }

    fn event(&self, name: &str, handler: Box<EventListener>) {
        crate::internal::event_internal(self.node.unchecked_ref(), name, handler)
    }

    fn update_text(&self, text: &str) {
        self.node
            .dyn_ref::<Text>()
            .unwrap()
            .set_text_content(Some(text));
    }
}
