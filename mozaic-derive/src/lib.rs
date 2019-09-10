extern crate proc_macro;
#[macro_use]
extern crate syn;
#[macro_use]
extern crate quote;

use proc_macro::TokenStream;
use std::collections::HashMap;

#[proc_macro_derive(MozaicEvent, attributes(mozaic_event))]
pub fn derive_mozaic_event(input: TokenStream) -> TokenStream {
    let input: syn::DeriveInput = syn::parse(input).unwrap();
    impl_mozaic_event(&input)
}

fn impl_mozaic_event(ast: &syn::DeriveInput) -> TokenStream {
    let event_ident = &ast.ident;

    let meta_items = get_meta_items(ast);
    let type_id: u32 = match meta_items.get("type_id").unwrap() {
        &syn::Lit::Str(ref lit_str) => {
            lit_str.value().parse().unwrap()
        }
        _ => panic!("expected type_id to be a string"),
    };

    let tokens = quote! {
        impl ::reactors::Event for #event_ident {
            const TYPE_ID: u32 = #type_id;
        }
    };
    return tokens.into();
}

fn get_meta_items(ast: &syn::DeriveInput) -> HashMap<String, syn::Lit> {
    let mut items = HashMap::new();

    for attr in ast.attrs.iter() {
        if path_equals(&attr.path, "mozaic_event") {
            let meta_list = match attr.parse_meta() {
                Err(_) => panic!("could not interpret meta"),
                Ok(syn::Meta::List(list)) => list,
                Ok(_) => panic!("expected MetaList"),
            };

            for nested_meta in meta_list.nested.iter() {
                let meta = match nested_meta {
                    &syn::NestedMeta::Lit(_) => panic!("unexpected literal"),
                    &syn::NestedMeta::Meta(ref meta) => meta,
                };

                match meta {
                    &syn::Meta::NameValue(ref name_value) => {
                        items.insert(
                            name_value.path.get_ident().unwrap().to_string(),
                            name_value.lit.clone(),
                        );
                    },
                    _ => panic!("expected NameValue")
                }
            }
        }
    }

    return items;
}

fn path_equals(path: &syn::Path, s: &str) -> bool {
    path.segments.len() == 1 && path.segments[0].ident == s
}

use syn::parse::{Result, ParseStream};

struct  Combinations {
    reader: syn::Type,
    owned: syn::Type,
}

impl syn::parse::Parse for Combinations {
    fn parse(input: ParseStream) -> Result<Self> {
        let reader = input.parse()?;
        input.parse::<syn::Token![,]>()?;
        let owned = input.parse()?;
        if !input.is_empty() {
            return Err(input.error("Did not consume entire parse stream"));
        }
        Ok(Combinations { reader, owned })
    }
}

#[proc_macro]
pub fn minimal(input: TokenStream) -> TokenStream {
    let Combinations { reader, owned } = parse_macro_input!(input as Combinations);
    (quote!{
        pub fn e_to_i<C: ::messaging::reactor::Ctx, T>(
            _: &mut T,
            h: &mut ::messaging::reactor::LinkHandle<C>,
            r: #reader) -> ::errors::Result<()>
        {
            let m = ::messaging::types::MsgBuffer::<#owned>::from_reader(r)?;
            h.send_internal(m)?;
            Ok(())
        }
    }).into()
}
