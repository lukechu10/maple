use proc_macro::TokenStream;
use syn::parse_macro_input;

mod component;
mod template;

/// A macro for ergonomically creating complex UI structures.
///
/// To learn more about the template syntax, see the chapter on
/// [the `template!` macro](https://sycamore-rs.netlify.app/docs/basics/template) in the Sycamore Book.
#[proc_macro]
pub fn template(component: TokenStream) -> TokenStream {
    let component = parse_macro_input!(component as template::HtmlRoot);

    template::template_impl(component).into()
}

/// ```
/// use sycamore::{generic_node::EventHandler, prelude::*};
///
/// #[component(MyComponent<G>)]
/// pub fn my_component(event_listeners: Vec<(String, Box<EventHandler>)>) -> Template<G> {
///     let cool_button: G = node! { button { "The coolest 😎" } };
///
///     for listener in event_listeners {
///         cool_button.event(listener.0.as_ref(), listener.1);
///     }
///
///     Template::new_node(cool_button)
/// }
/// ```
#[proc_macro]
pub fn node(input: TokenStream) -> TokenStream {
    let node = parse_macro_input!(input as template::Element);

    template::node_impl(node).into()
}

/// A macro for creating components from functions.
///
/// Add this attribute to a `fn` to create a component from that function.
///
/// To learn more about components, see the chapter on
/// [components](https://sycamore-rs.netlify.app/docs/basics/components) in the Sycamore Book.
#[proc_macro_attribute]
pub fn component(attr: TokenStream, component: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as component::ComponentFunctionName);
    let component = parse_macro_input!(component as component::ComponentFunction);

    component::component_impl(attr, component)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
