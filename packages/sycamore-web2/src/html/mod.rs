//! HTML and SVG tag definitions.
//!
//! _Documentation sources: <https://developer.mozilla.org/en-US/>_

pub mod elements;

mod attributes;
mod bind_props;
mod events;
mod props;

use std::cell::RefCell;
use std::fmt;

pub use attributes::{GlobalAttributes, HtmlGlobalAttributes, SvgGlobalAttributes};
pub use bind_props::{bind, BindAttributes};
pub use elements::*;
pub use events::{on, OnAttributes};
pub use props::{prop, PropAttributes};
use sycamore_core2::elements::Spread;
use sycamore_reactive::Scope;

use crate::web_node::WebNode;
use crate::ElementBuilder;

type AttrFn<'a, E> = Box<dyn FnOnce(ElementBuilder<E>) + 'a>;

/// A struct that can keep track of the attributes that are added.
/// This can be used as a prop to a component to allow the component to accept arbitrary attributes
/// and then spread them onto the element.
pub struct Attributes<'a, E: WebElement> {
    fns: RefCell<Vec<AttrFn<'a, E>>>,
}

impl<'a, E: WebElement> Attributes<'a, E> {
    /// Create a new instance of [`Attributes`].
    pub fn new() -> Self {
        Self {
            fns: RefCell::new(Vec::new()),
        }
    }

    /// Add a closure.
    pub fn add_fn<F>(&self, f: F)
    where
        F: FnOnce(ElementBuilder<E>) + 'static,
    {
        self.fns.borrow_mut().push(Box::new(f));
    }

    /// Apply all the attributes to the element builder.
    pub fn apply(self, builder: ElementBuilder<E>) {
        for f in self.fns.into_inner() {
            f(builder.clone());
        }
    }
}

impl<'a, E: WebElement> Default for Attributes<'a, E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, E: WebElement> fmt::Debug for Attributes<'a, E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Attributes").finish()
    }
}

impl<'a, E: WebElement> Spread<E, WebNode> for Attributes<'a, E> {
    fn spread(self, cx: Scope, el: &WebNode) {
        self.apply(ElementBuilder::from_element(cx, E::from_node(el.clone())));
    }
}
