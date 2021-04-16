use proc_macro2::TokenStream;
use quote::{quote_spanned, ToTokens};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::{Comma, Paren};
use syn::{parenthesized, Expr, Path, Result};

/// Components are identical to function calls.
pub struct Component {
    pub path: Path,
    pub paren: Paren,
    pub args: Punctuated<Expr, Comma>,
}

impl Parse for Component {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        Ok(Self {
            path: input.parse()?,
            paren: parenthesized!(content in input),
            args: content.parse_terminated(Expr::parse)?,
        })
    }
}

impl ToTokens for Component {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Component { path, paren, args } = self;

        let quoted = if args.empty_or_trailing() {
            quote_spanned! { paren.span=>
                ::maple_core::reactive::untrack(||
                    #path::<_>::__create_component(())
                )
            }
        } else {
            quote_spanned! { path.span()=>
                ::maple_core::reactive::untrack(||
                    #path::<_>::__create_component(#args)
                )
            }
        };

        tokens.extend(quoted);
    }
}
