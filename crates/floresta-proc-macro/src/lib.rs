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
use syn::Error;
use syn::ItemEnum;
use syn::parse_macro_input;

use crate::enum_str_map::FormattedEnumOptions;

mod enum_str_map;

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
