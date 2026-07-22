use quote::format_ident;
use quote::quote;
use syn::Error;
use syn::Ident;
use syn::ItemEnum;
use syn::Result;
use syn::Token;
use syn::parse::Parse;
use syn::parse::ParseStream;

pub(crate) fn expand_formatted_enum(
    options: FormattedEnumOptions,
    input: ItemEnum,
) -> Result<proc_macro2::TokenStream> {
    let enum_name = input.ident;
    let vis = input.vis;
    let attrs = input.attrs;
    let variants = input
        .variants
        .into_iter()
        .map(|variant| variant.ident)
        .collect::<Vec<_>>();

    expand_enum_and_impls(
        enum_name,
        Some((attrs, vis)),
        variants,
        options.separator,
        options.case,
    )
}

fn expand_enum_and_impls(
    enum_name: Ident,
    existing_enum: Option<(Vec<syn::Attribute>, syn::Visibility)>,
    variants: Vec<Ident>,
    separator: String,
    transform: CaseTransform,
) -> Result<proc_macro2::TokenStream> {
    let method_names = variants
        .iter()
        .map(|variant| transform_variant_name(variant, &transform, &separator))
        .collect::<Vec<_>>();

    let enum_definition = if let Some((attrs, vis)) = existing_enum {
        quote! {
            #(#attrs)*
            #vis enum #enum_name {
                #(#variants),*
            }
        }
    } else {
        quote! {
            #[derive(Debug, Clone, PartialEq, Eq)]
            pub enum #enum_name {
                #(#variants),*
            }
        }
    };

    Ok(quote! {
        #enum_definition

        impl ::core::str::FromStr for #enum_name {
            type Err = ::std::string::String;

            fn from_str(s: &str) -> ::core::result::Result<Self, Self::Err> {
                match s {
                    #(#method_names => Ok(Self::#variants),)*
                    _ => Err(::std::format!("Unknown method: {}", s)),
                }
            }
        }

        impl #enum_name {
            pub fn to_string(&self) -> ::std::string::String {
                self.as_str().to_owned()
            }

            pub fn as_str(&self) -> &'static str {
                match self {
                    #(Self::#variants => #method_names,)*
                }
            }
        }

        impl ::std::ops::Deref for #enum_name {
            type Target = str;

            fn deref(&self) -> &str {
                self.as_str()
            }
        }
    })
}

#[derive(Clone)]
enum CaseTransform {
    Lower,
    Upper,
    Preserve,
}

/// Options for the `#[formatted_enum]` macro.
///
/// - `case`: Transform case of variant names (lower, upper, preserve)
/// - `separator`: String to insert between PascalCase words
pub(crate) struct FormattedEnumOptions {
    case: CaseTransform,
    separator: String,
}

impl Default for FormattedEnumOptions {
    fn default() -> Self {
        Self {
            case: CaseTransform::Lower,
            separator: String::new(),
        }
    }
}

impl Parse for FormattedEnumOptions {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut options = FormattedEnumOptions::default();

        while !input.is_empty() {
            let option_name: Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            if option_name == format_ident!("case") {
                let value: syn::LitStr = input.parse()?;
                options.case = match value.value().as_str() {
                    "lower" => CaseTransform::Lower,
                    "upper" => CaseTransform::Upper,
                    "preserve" => CaseTransform::Preserve,
                    _ => {
                        return Err(Error::new_spanned(
                            value,
                            "case must be one of: lower, upper, preserve",
                        ));
                    }
                };
            } else if option_name == format_ident!("separator") {
                let value: syn::LitStr = input.parse()?;
                options.separator = value.value();
            } else {
                return Err(Error::new_spanned(
                    option_name,
                    "unknown option, expected case or separator",
                ));
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(options)
    }
}

/// Helper function to convert PascalCase variant names to formatted strings.
///
/// Splits on uppercase boundaries and applies case transformation.
/// Example: `GetBestBlockHash` with separator="." and case="lower"
/// becomes `["Get", "Best", "Block", "Hash"]` joined as "get.best.block.hash"
fn transform_variant_name(
    variant: &Ident,
    case: &CaseTransform,
    separator: &str,
) -> proc_macro2::Literal {
    let words = split_pascal_case(&variant.to_string());
    let transformed = words
        .into_iter()
        .map(|word| match case {
            CaseTransform::Lower => word.to_lowercase(),
            CaseTransform::Upper => word.to_uppercase(),
            CaseTransform::Preserve => word,
        })
        .collect::<Vec<_>>()
        .join(separator);

    proc_macro2::Literal::string(&transformed)
}

/// Splits a PascalCase string into individual words.
///
/// Splits whenever an uppercase letter is encountered (except at position 0).
/// Example: "GetBestBlockHash" → ["Get", "Best", "Block", "Hash"]
fn split_pascal_case(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();

    for (idx, ch) in input.chars().enumerate() {
        if idx > 0 && ch.is_uppercase() && !current.is_empty() {
            words.push(::core::mem::take(&mut current));
        }
        current.push(ch);
    }

    if !current.is_empty() {
        words.push(current);
    }

    words
}
