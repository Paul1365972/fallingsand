mod dsl;
mod emit;
mod model;

use proc_macro::TokenStream;

#[proc_macro]
pub fn content(input: TokenStream) -> TokenStream {
    match expand_content(input.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

fn expand_content(input: proc_macro2::TokenStream) -> syn::Result<proc_macro2::TokenStream> {
    let header = syn::parse2::<dsl::Header>(input)?;
    let sources = dsl::read_sources(&header)?;
    let content = model::build(&header, &sources)?;
    Ok(emit::emit(&header, &content))
}

#[proc_macro]
pub fn per_tick_threshold(input: TokenStream) -> TokenStream {
    match expand_per_tick_threshold(input.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.into_compile_error().into(),
    }
}

fn expand_per_tick_threshold(
    input: proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let rate = syn::parse2::<syn::LitFloat>(input)?.base10_parse::<f32>()?;
    let threshold =
        fallingsand_material::chance_threshold(fallingsand_material::per_tick_chance(rate));
    let literal = proc_macro2::Literal::u64_suffixed(threshold);
    Ok(quote::quote!(#literal))
}
