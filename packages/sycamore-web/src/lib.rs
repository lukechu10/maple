pub mod bind;
#[cfg(feature = "dom")]
mod dom;
mod elements;
pub mod events;
mod iter;
mod node;
mod noderef;
mod portal;
#[cfg(feature = "ssr")]
mod ssr;
mod view;

use std::any::{Any, TypeId};
use std::borrow::Cow;
use std::cell::{OnceCell, RefCell};
use std::rc::Rc;

#[cfg(feature = "dom")]
pub use dom::*;
pub use elements::*;
pub use iter::*;
pub use node::*;
pub use noderef::*;
pub use portal::*;
#[cfg(feature = "ssr")]
pub use ssr::*;
use sycamore_reactive::*;
use wasm_bindgen::JsCast;

/// We add this to make the macros from `sycamore-macro` work properly.
extern crate self as sycamore;
mod rt {
    pub use sycamore_core::*;
    pub use sycamore_macro::*;

    pub use crate::*;
}

/// A type alias for [`View`](self::view::View) with [`HtmlNode`] as the node type.
pub type View = self::view::View<HtmlNode>;
/// A type alias for [`Children`](sycamore_core::Children) with [`HtmlNode`] as the node type.
pub type Children = sycamore_core::Children<View>;

/// A struct for keeping track of state used for hydration.
#[derive(Debug, Clone, Copy)]
struct HydrationRegistry {
    next_key: Signal<u32>,
}

impl HydrationRegistry {
    pub fn new() -> Self {
        HydrationRegistry {
            next_key: create_signal(0),
        }
    }

    /// Get the next hydration key and increment the internal state. This new key will be unique.
    pub fn next_key(self) -> u32 {
        let key = self.next_key.get();
        self.next_key.set(key + 1);
        key
    }
}

impl Default for HydrationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Marker struct to be inserted into reactive context to indicate that we are in SSR mode.
#[derive(Clone, Copy)]
struct SsrMode;

/// Returns whether we are in SSR mode or not.
pub fn is_ssr() -> bool {
    if cfg!(feature = "dom") && !cfg!(feature = "ssr") {
        false
    } else if cfg!(feature = "ssr") && !cfg!(feature = "dom") {
        true
    } else {
        // Do a runtime check.
        try_use_context::<SsrMode>().is_some()
    }
}

/// Returns whether we are in client side rendering (CSR) mode or not.
///
/// This is the opposite of [`is_ssr`].
pub fn is_client() -> bool {
    !is_ssr()
}

/// Create a new effect, but only if we are not in SSR mode.
pub fn create_client_effect(f: impl FnMut() + 'static) {
    if !is_ssr() {
        create_effect(f);
    }
}

#[sycamore_macro::component]
fn Test() -> View {
    let checked = create_signal(true);
    sycamore_macro::view! {
        div(class="test", on:click=|_| todo!(), prop:value=1) {
            "hello, world!"
            button(bind:checked=checked)
            Test()
        }
    }
}
