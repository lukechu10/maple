//! # `sycamore-web`
//!
//! Web rendering backend for [`sycamore`](https://docs.rs/sycamore). This is already re-exported
//! in the main `sycamore` crate, so you should rarely need to use this crate directly.
//!
//! ## Feature flags
//!
//! - `dom` (_default_) - Enables the DOM rendering backend.
//!
//! - `ssr` - Enables server-side rendering (SSR) support.
//!
//! - `wasm-bindgen-interning` (_default_) - Enables interning for `wasm-bindgen` strings. This
//!   improves performance at a slight cost in binary size. If you want to minimize the size of the
//!   resulting `.wasm` binary, you might want to disable this.

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
use std::cell::{Cell, OnceCell, RefCell};
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
use wasm_bindgen::prelude::*;

/// We add this to make the macros from `sycamore-macro` work properly.
extern crate self as sycamore;
#[doc(hidden)]
#[allow(ambiguous_glob_reexports)]
pub mod rt {
    pub use sycamore_core::*;
    pub use sycamore_macro::*;
    #[allow(unused_imports)] // Needed for macro support.
    pub use web_sys;

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

/// Queue up a callback to be executed when the component is mounted.
///
/// If not on `wasm32` target, does nothing.
///
/// # Potential Pitfalls
///
/// If called inside an async-component, the callback will be called after the next suspension
/// point (when there is an `.await`).
pub fn on_mount(f: impl FnOnce() + 'static) {
    if cfg!(target_arch = "wasm32") {
        let is_alive = Rc::new(Cell::new(true));
        on_cleanup({
            let is_alive = Rc::clone(&is_alive);
            move || is_alive.set(false)
        });

        let scope = use_current_scope();
        let cb = move || {
            if is_alive.get() {
                scope.run_in(f);
            }
        };
        queue_microtask(cb);
    }
}

/// Alias for `queueMicrotask`.
pub fn queue_microtask(f: impl FnOnce() + 'static) {
    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_name = "queueMicrotask")]
        fn queue_microtask_js(f: &wasm_bindgen::JsValue);
    }
    queue_microtask_js(&Closure::once_into_js(f));
}

/// Utility function for accessing the global [`web_sys::Window`] object.
pub fn window() -> web_sys::Window {
    web_sys::window().expect("no global `window` exists")
}

/// Utility function for accessing the global [`web_sys::Document`] object.
pub fn document() -> web_sys::Document {
    thread_local! {
        /// Cache for small performance improvement by preventing repeated calls to `window().document()`.
        static DOCUMENT: web_sys::Document = window().document().expect("no `document` exists");
    }
    DOCUMENT.with(Clone::clone)
}

/// Log a message to the JavaScript console if on wasm32. Otherwise logs it to stdout.
///
/// Note: this does not work properly for server-side WASM since it will mistakenly try to log to
/// the JS console.
#[macro_export]
macro_rules! console_log {
    ($($arg:tt)*) => {
        if cfg!(target_arch = "wasm32") {
            $crate::rt::web_sys::console::log_1(&::std::format!($($arg)*).into());
        } else {
            ::std::println!($($arg)*);
        }
    };
}

/// Prints an error message to the JavaScript console if on wasm32. Otherwise logs it to stderr.
///
/// Note: this does not work properly for server-side WASM since it will mistakenly try to log to
/// the JS console.
#[macro_export]
macro_rules! console_error {
    ($($arg:tt)*) => {
        if cfg!(target_arch = "wasm32") {
            $crate::rt::web_sys::console::error_1(&::std::format!($($arg)*).into());
        } else {
            ::std::eprintln!($($arg)*);
        }
    };
}

/// Debug the value of a variable to the JavaScript console if on wasm32. Otherwise logs it to
/// stdout.
///
/// Note: this does not work properly for server-side WASM since it will mistakenly try to log to
/// the JS console.
#[macro_export]
macro_rules! console_dbg {
    () => {
        if cfg!(target_arch = "wasm32") {
            $crate::rt::web_sys::console::log_1(
                &::std::format!("[{}:{}]", ::std::file!(), ::std::line!(),).into(),
            );
        } else {
            ::std::dbg!($arg);
        }
    };
    ($arg:expr $(,)?) => {
        if cfg!(target_arch = "wasm32") {
            $crate::rt::web_sys::console::log_1(
                &::std::format!(
                    "[{}:{}] {} = {:#?}",
                    ::std::file!(),
                    ::std::line!(),
                    ::std::stringify!($arg),
                    $arg
                )
                .into(),
            );
        } else {
            ::std::dbg!($arg);
        }
    };
    ($($arg:expr),+ $(,)?) => {
        $($crate::console_dbg!($arg);)+
    }
}
