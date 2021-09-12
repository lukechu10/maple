use sycamore_reactive::{create_root, ReactiveScope};
use wasm_bindgen::JsCast;
use web_sys::{Element, Node};

use crate::template::Template;
use crate::utils::render::insert;
use crate::DomNode;

/// Render a [`Template`] under a `parent` node by reusing existing nodes (client side
/// hydration). Alias for [`hydrate_to`] with `parent` being the `<body>` tag.
///
/// For rendering without hydration, use [`render`] instead.
///
/// **TODO**: This method currently deletes existing nodes from DOM and reinserts new
/// created nodes. This will be fixed in a later release.
///
/// _This API requires the following crate features to be activated: `dom` and `hydrate`_
pub fn hydrate(template: impl FnOnce() -> Template<DomNode>) {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();

    hydrate_to(template, &document.body().unwrap());
}

/// Render a [`Template`] under a `parent` node by reusing existing nodes (client side
/// hydration). For rendering under the `<body>` tag, use [`hydrate_to`] instead.
///
/// For rendering without hydration, use [`render`] instead.
///
/// **TODO**: This method currently deletes existing nodes from DOM and reinserts new
/// created nodes. This will be fixed in a later release.
///
/// _This API requires the following crate features to be activated: `dom` and `hydrate`_
pub fn hydrate_to(template: impl FnOnce() -> Template<DomNode>, parent: &Node) {
    parent.unchecked_ref::<Element>().set_inner_html("");

    let scope = create_root(|| {
        insert(
            &DomNode::new(parent.clone()),
            template(),
            None,
            None, // TODO
            false,
        );
    });

    thread_local! {
        static GLOBAL_SCOPES: std::cell::RefCell<Vec<ReactiveScope>> = std::cell::RefCell::new(Vec::new());
    }

    GLOBAL_SCOPES.with(|global_scopes| global_scopes.borrow_mut().push(scope));
}
