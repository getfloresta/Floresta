// SPDX-License-Identifier: MIT OR Apache-2.0

// cargo docs customization
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(html_logo_url = "https://avatars.githubusercontent.com/u/249173822")]
#![doc(
    html_favicon_url = "https://raw.githubusercontent.com/getfloresta/floresta-media/master/logo_png/Icon-Green(main).png"
)]
#![allow(clippy::manual_is_multiple_of)]
#![cfg_attr(not(test), deny(clippy::as_conversions))]

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::Error;
use syn::ItemEnum;
use syn::ItemFn;
use syn::ItemTrait;
use syn::ReturnType;
use syn::parse_macro_input;

use crate::enum_str_map::FormattedEnumOptions;

mod enum_str_map;

/// A procedural macro attribute that makes a trait sync or async based on the 'async' feature flag.
///
/// When the 'async' feature is enabled, method return types are transformed to return
/// `impl Future<Output = T>` instead of just `T`.
///
/// # Example
///
/// ```ignore
/// use maybe_async::maybe_async;
///
/// #[maybe_async]
/// trait Foo {
///     fn method(&self, a: String) -> Result<(), String>;
/// }
/// ```
///
/// With the 'async' feature enabled, this will expand to:
///
/// ```ignore
/// use std::future::Future;
///
/// trait Foo {
///     fn method(&self, a: String) -> impl Future<Output = Result<(), String>>;
/// }
/// ```
///
/// Without the 'async' feature, the trait will be unchanged.
#[proc_macro_attribute]
pub fn maybe_async(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the input token stream into a trait item
    let input = parse_macro_input!(item as ItemTrait);

    // If the async feature is enabled, transform the trait methods
    #[cfg(feature = "async")]
    {
        let mut modified_items = Vec::new();

        for item in input.items {
            if let syn::TraitItem::Fn(mut method) = item {
                // Transform the return type to wrap it in `impl Future<Output = ...>`
                if let syn::ReturnType::Type(arrow, return_type) = method.sig.output {
                    let new_return_type = quote! {
                        impl ::core::future::Future<Output = #return_type>
                    };

                    method.sig.output = syn::ReturnType::Type(
                        arrow,
                        Box::new(syn::parse2(new_return_type).unwrap()),
                    );
                }
                modified_items.push(syn::TraitItem::Fn(method));
            } else {
                modified_items.push(item);
            }
        }

        let input = ItemTrait {
            items: modified_items,
            ..input
        };

        let output = quote! {
            #input
        };

        output.into()
    }
    #[cfg(not(feature = "async"))]
    {
        // Without the async feature, just return the original trait
        let output = quote! {
            #input
        };

        output.into()
    }
}

/// A helper procedural macro that transforms an async function into a sync function
/// that returns `impl Future<Output = T>`.
///
/// This is useful when implementing traits transformed by `maybe_async` in async mode.
/// Instead of writing the implementation with explicit future returns, you can write
/// it with the `async` keyword and this macro will handle the transformation.
///
/// # Example
///
/// ```ignore
/// #[to_future]
/// async fn my_function(arg: String) -> Result<String, Error> {
///     // Async code here...
///     Ok("result".to_string())
/// }
/// ```
///
/// Will be transformed into:
///
/// ```ignore
/// fn my_function(arg: String) -> impl Future<Output = Result<String, Error>> {
///     async move {
///         // Async code here...
///         Ok("result".to_string())
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn to_future(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);

    // Extract function information
    let vis = &input.vis;
    let sig = &input.sig;
    let block = &input.block;
    let attrs = &input.attrs;

    // Check if the function is already async, error if not
    if sig.asyncness.is_none() {
        let error = quote! {
            compile_error!("The #[to_future] attribute can only be used on async functions");
        };
        return TokenStream::from(error);
    }

    // Extract output type
    let return_type = match &sig.output {
        ReturnType::Default => quote! { () },
        ReturnType::Type(_, ty) => quote! { #ty },
    };

    // Create a new function signature without the async keyword
    let fn_name = &sig.ident;
    let generics = &sig.generics;
    let inputs = &sig.inputs;

    let output = quote! {
        #(#attrs)*
        #vis fn #fn_name #generics(#inputs) -> impl ::core::future::Future<Output = #return_type> {
            async move #block
        }
    };

    output.into()
}

/// Generates a bidirectional mapping between enum variants and strings.
///
/// This attribute macro implements string conversion helpers for fieldless enums:
/// - parsing from string to enum via FromStr
/// - conversion from enum to string via as_str and to_string
/// - Deref<Target = str> for ergonomic string access
///
/// # Options
///
/// - `case = "lower" | "upper" | "preserve"`: Transform variant names (default: "lower")
/// - `separator = "..."`: String to join PascalCase words (default: empty string)
///
/// # Generated Implementations
///
/// The macro generates:
/// - `FromStr` trait for parsing strings back to enum variants
/// - `as_str()` method returning `&'static str`
/// - `to_string()` method for owned `String` conversion
/// - `Deref<Target = str>` for seamless string interoperability
///
/// # Examples
///
/// Lower + separator "."
///
/// ```rust
/// use std::str::FromStr;
///
/// use florersta_macro::enum_str_map;
///
/// #[enum_str_map(case = "lower", separator = ".")]
/// #[derive(Debug, Clone, PartialEq, Eq)]
/// enum RpcMethodLower {
///     GetBestBlockHash,
///     FindTransaction,
/// }
///
/// assert_eq!(
///     RpcMethodLower::GetBestBlockHash.as_str(),
///     "get.best.block.hash"
/// );
/// assert_eq!(
///     RpcMethodLower::FindTransaction.to_string(),
///     "find.transaction"
/// );
/// assert_eq!(
///     RpcMethodLower::from_str("get.best.block.hash").unwrap(),
///     RpcMethodLower::GetBestBlockHash
/// );
/// ```
///
/// No arguments (defaults to lower + no separator)
///
/// ```rust
/// use std::str::FromStr;
///
/// use florersta_macro::enum_str_map;
///
/// #[enum_str_map]
/// #[derive(Debug, Clone, PartialEq, Eq)]
/// enum RpcMethodDefault {
///     GetBestBlockHash,
/// }
///
/// assert_eq!(
///     RpcMethodDefault::GetBestBlockHash.as_str(),
///     "getbestblockhash"
/// );
/// assert_eq!(
///     RpcMethodDefault::from_str("getbestblockhash").unwrap(),
///     RpcMethodDefault::GetBestBlockHash
/// );
/// ```
///
/// Case only (no separator)
///
/// ```rust
/// use std::str::FromStr;
///
/// use florersta_macro::enum_str_map;
///
/// #[enum_str_map(case = "upper")]
/// #[derive(Debug, Clone, PartialEq, Eq)]
/// enum RpcMethodCaseOnly {
///     GetBestBlockHash,
/// }
///
/// assert_eq!(
///     RpcMethodCaseOnly::GetBestBlockHash.as_str(),
///     "GETBESTBLOCKHASH"
/// );
/// assert_eq!(
///     RpcMethodCaseOnly::from_str("GETBESTBLOCKHASH").unwrap(),
///     RpcMethodCaseOnly::GetBestBlockHash
/// );
/// ```
///
/// Separator only (no case transformation)
///
/// ```rust
/// use std::str::FromStr;
/// use florersta_macro::enum_str_map;
///
/// #[enum_str_map(separator = "_")]
/// #[derive(Debug, Clone, PartialEq, Eq)]
/// enum RpcMethodSepOnly {
///     GetBestBlockHash,
/// }
///
/// assert_eq!(RpcMethodSepOnly::GetBestBlockHash.as_str(), "get_best_block_hash");
/// assert_eq!(
///     RpcMethodSepOnly::from_str("get_best_block_hash").unwrap(),
///     RpcMethodSepOnly::GetBestBlockHash
/// );
#[proc_macro_attribute]
pub fn enum_str_map(attr: TokenStream, item: TokenStream) -> TokenStream {
    let options = parse_macro_input!(attr as FormattedEnumOptions);
    let input = parse_macro_input!(item as ItemEnum);
    enum_str_map::expand_formatted_enum(options, input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}
