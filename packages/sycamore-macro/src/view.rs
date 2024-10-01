//! Codegen for `view!` macro.
//!
//! Implementation note: We are not using the `quote::ToTokens` trait because we need to pass
//! additional information to the codegen such as which mode (Client, Hydrate, SSR), etc...

use proc_macro2::TokenStream;
use quote::quote;
use sycamore_view_parser::ir::{DynNode, Node, Prop, PropType, Root, TagIdent, TagNode, TextNode};
use syn::{Expr, Pat};

pub struct Codegen {
    // TODO: configure mode: Client, Hydrate, SSR
}

impl Codegen {
    pub fn root(&self, root: &Root) -> TokenStream {
        match &root.0[..] {
            [] => quote! {
                ::sycamore::rt::View::new()
            },
            [node] => self.node(node),
            nodes => {
                let nodes = nodes.iter().map(|node| self.node(node));
                quote! {
                    ::std::convert::Into::<::sycamore::rt::View>::into(::std::vec![#(#nodes),*])
                }
            }
        }
    }

    /// Generate a `View` from a `Node`.
    pub fn node(&self, node: &Node) -> TokenStream {
        match node {
            Node::Tag(tag) => {
                if is_component(&tag.ident) {
                    self.component(tag)
                } else {
                    self.element(tag)
                }
            }
            Node::Text(TextNode { value }) => quote! {
                ::std::convert::Into::<::sycamore::rt::View>::into(#value)
            },
            Node::Dyn(DynNode { value }) => {
                let is_dynamic = is_dyn(value);
                if is_dynamic {
                    quote! {
                        ::sycamore::rt::View::from_dynamic(
                            move || ::std::convert::Into::<::sycamore::rt::View>::into(#value)
                        )
                    }
                } else {
                    quote! {
                        ::std::convert::Into::<::sycamore::rt::View>::into(#value)
                    }
                }
            }
        }
    }

    pub fn element(&self, element: &TagNode) -> TokenStream {
        let TagNode {
            ident,
            props,
            children,
        } = element;

        let attributes = props.iter().map(|attr| self.attribute(attr));

        let children = children
            .0
            .iter()
            .map(|child| self.node(child))
            .collect::<Vec<_>>();

        match ident {
            TagIdent::Path(tag) => {
                assert!(tag.get_ident().is_some(), "elements must be an ident");
                quote! {
                    ::sycamore::rt::View::from(
                        ::sycamore::rt::tags::#tag().children(::std::vec![#(#children),*])#(#attributes)*
                    )
                }
            }
            TagIdent::Hyphenated(tag) => quote! {
                ::sycamore::rt::View::from(
                    ::sycamore::rt::custom_element(#tag).children(::std::vec![#(#children),*])#(#attributes)*
                )
            },
        }
    }

    pub fn attribute(&self, attr: &Prop) -> TokenStream {
        let value = &attr.value;
        let is_dynamic = !matches!(value, Expr::Lit(_) | Expr::Closure(_) | Expr::Path(_));
        let dyn_value = if is_dynamic {
            quote! { move || #value }
        } else {
            quote! { #value }
        };
        match &attr.ty {
            PropType::Plain { ident } => {
                quote! { .#ident(#dyn_value) }
            }
            PropType::PlainHyphenated { ident } => {
                quote! { .attr(#ident, #dyn_value) }
            }
            PropType::PlainQuoted { ident } => {
                quote! { .attr(#ident, #dyn_value) }
            }
            PropType::Directive { dir, ident } => match dir.to_string().as_str() {
                "on" => quote! { .on(::sycamore::rt::events::#ident, #value) },
                "prop" => {
                    let ident = ident.to_string();
                    quote! { .prop(#ident, #dyn_value) }
                }
                "bind" => quote! { .bind(::sycamore::rt::bind::#ident, #value) },
                _ => syn::Error::new(dir.span(), format!("unknown directive `{dir}`"))
                    .to_compile_error(),
            },
            PropType::Ref => quote! { .r#ref(#value) },
            PropType::Spread => quote! { .spread(#value) },
        }
    }

    pub fn component(
        &self,
        TagNode {
            ident,
            props,
            children,
        }: &TagNode,
    ) -> TokenStream {
        let ident = match ident {
            TagIdent::Path(path) => path,
            TagIdent::Hyphenated(_) => unreachable!("hyphenated tags are not components"),
        };

        let plain = props
            .iter()
            .filter_map(|prop| match &prop.ty {
                PropType::Plain { ident } => Some((ident, prop.value.clone())),
                _ => None,
            })
            .collect::<Vec<_>>();
        let plain_names = plain.iter().map(|(ident, _)| ident);
        let plain_values = plain.iter().map(|(_, value)| value);

        let other_props = props
            .iter()
            .filter(|prop| !matches!(&prop.ty, PropType::Plain { .. }))
            .collect::<Vec<_>>();
        let other_attributes = other_props.iter().map(|prop| self.attribute(prop));

        let children_quoted = if children.0.is_empty() {
            quote! {}
        } else {
            let codegen = Codegen {};
            let children = codegen.root(children);
            quote! {
                .children(
                    ::sycamore::rt::Children::new(move || {
                        #children
                    })
                )
            }
        };
        quote! {{
            let __component = &#ident; // We do this to make sure the compiler can infer the value for `<G>`.
            ::sycamore::rt::component_scope(move || ::sycamore::rt::Component::create(
                __component,
                ::sycamore::rt::element_like_component_builder(__component)
                    #(.#plain_names(#plain_values))*
                    #(#other_attributes)*
                    #children_quoted
                    .build()
            ))
        }}
    }
}

fn is_component(ident: &TagIdent) -> bool {
    match ident {
        TagIdent::Path(path) => {
            path.get_ident().is_none()
                || path
                    .get_ident()
                    .unwrap()
                    .to_string()
                    .chars()
                    .next()
                    .unwrap()
                    .is_ascii_uppercase()
        }
        // A hyphenated tag is always a custom-element and therefore never a component.
        TagIdent::Hyphenated(_) => false,
    }
}

fn is_dyn(ex: &Expr) -> bool {
    match ex {
        Expr::Lit(_) | Expr::Closure(_) | Expr::Path(_) | Expr::Field(_) => false,

        Expr::Tuple(t) => t.elems.iter().any(|e| is_dyn(e)),
        Expr::Array(a) => a.elems.iter().any(|e| is_dyn(e)),
        Expr::Struct(s) => s.fields.iter().any(|fv: &syn::FieldValue| is_dyn(&fv.expr)),

        Expr::Match(m) => {
            is_dyn(&m.expr)
                || m.arms.iter().any(|a: &syn::Arm| {
                    is_dyn_pattern(&a.pat)
                        || a.guard.as_ref().is_some_and(|(_, g_expr)| is_dyn(g_expr))
                        || is_dyn(&a.body)
                })
        }

        Expr::Index(i) => is_dyn(&i.expr) || is_dyn(&i.index),
        Expr::Call(c) => c.args.iter().any(|ex| is_dyn(ex)),
        Expr::MethodCall(mc) => is_dyn(&mc.receiver) || mc.args.iter().any(|arg| is_dyn(arg)),

        Expr::Unary(u) => is_dyn(&u.expr),
        Expr::Cast(c) => is_dyn(&c.expr),
        Expr::Paren(p) => is_dyn(&p.expr),

        // TODO
        Expr::Block(_b) => true,

        // Don't descend into nested inner view! macros, because their bodies
        // will be checked for dynamic parts when their own codegen is run.
        //
        // As for other macros: we have no idea what they could generate from
        // their TokenStreams, so lets assume those all are dynamic
        Expr::Macro(m) => !m
            .mac
            .path
            .get_ident()
            .is_some_and(|ident| "view" == &ident.to_string()),

        // TODO
        _ => true,
    }
}

fn is_dyn_pattern(pat: &Pat) -> bool {
    match pat {
        Pat::Wild(_) | Pat::Lit(_) | Pat::Path(_) | Pat::Rest(_) | Pat::Type(_) => false,

        Pat::Paren(p) => is_dyn_pattern(&p.pat),
        Pat::Tuple(t) => t.elems.iter().any(|p| is_dyn_pattern(p)),
        Pat::TupleStruct(s) => s.elems.iter().any(|e| is_dyn_pattern(e)),
        Pat::Struct(s) => s
            .fields
            .iter()
            .any(|fp: &syn::FieldPat| is_dyn_pattern(&fp.pat)),

        Pat::Reference(r) => r.mutability.is_some(),
        // TODO
        Pat::Ident(id) => id.by_ref.is_some() && id.mutability.is_some(),

        // TODO
        Pat::Const(_) => true,
        // TODO
        Pat::Or(_) => true,
        // TODO
        Pat::Range(_) => true,
        // TODO
        Pat::Slice(_) => true,

        // Don't mess with these, assume they are always dynamic
        Pat::Macro(_) => true,
        Pat::Verbatim(_) => true,

        // Need this, because Pat is marked as non-exhaustive
        _ => panic!("Unhandled syn::Pat variant"),
    }
}
