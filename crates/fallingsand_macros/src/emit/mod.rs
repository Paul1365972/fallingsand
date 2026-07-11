mod accessors;
mod specs;

use crate::dsl::Header;
use crate::model::Content;
use fallingsand_material::{MaterialId, Phase, Tags};
use proc_macro2::{Literal, TokenStream};
use quote::quote;

pub fn emit(header: &Header, content: &Content) -> TokenStream {
    let accessors = accessors::emit(content);
    let specs = specs::emit(content);
    let files = header
        .material_files
        .iter()
        .chain(std::iter::once(&header.reactions_file));

    quote! {
        #(const _: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/content/", #files));)*

        #accessors
        #specs
    }
}

pub(crate) fn material_id(id: MaterialId) -> TokenStream {
    let literal = Literal::u16_suffixed(id.0);
    quote!(crate::material::MaterialId(#literal))
}

pub(crate) fn phase_path(phase: Phase) -> TokenStream {
    match phase {
        Phase::Empty => quote!(crate::material::Phase::Empty),
        Phase::Solid => quote!(crate::material::Phase::Solid),
        Phase::Powder => quote!(crate::material::Phase::Powder),
        Phase::Liquid => quote!(crate::material::Phase::Liquid),
        Phase::Gas => quote!(crate::material::Phase::Gas),
    }
}

pub(crate) fn tags_tokens(tags: Tags) -> TokenStream {
    let bits = Literal::u32_suffixed(tags.bits());
    quote!(crate::material::Tags::from_bits(#bits))
}

pub(crate) fn colors_tokens(colors: &[[u8; 4]]) -> TokenStream {
    let rows = colors.iter().map(|[r, g, b, a]| quote!([#r, #g, #b, #a]));
    quote!(&[#(#rows),*])
}

pub(crate) fn accessor_fn(
    name: &str,
    return_type: TokenStream,
    values: impl Iterator<Item = TokenStream>,
    constant: bool,
) -> TokenStream {
    let constness = if constant { quote!(const) } else { quote!() };
    let name = proc_macro2::Ident::new(name, proc_macro2::Span::call_site());
    let arms = values.enumerate().map(|(index, value)| {
        let index = Literal::u16_suffixed(index as u16);
        quote!(#index => #value)
    });
    quote! {
        #[inline]
        pub #constness fn #name(id: crate::material::MaterialId) -> #return_type {
            match id.0 {
                #(#arms,)*
                _ => unreachable!(),
            }
        }
    }
}
