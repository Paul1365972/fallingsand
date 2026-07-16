use super::material_id;
use crate::model::{Content, ItemOut, RecipeOut};
use proc_macro2::{Literal, TokenStream};
use quote::quote;

pub fn emit(content: &Content) -> TokenStream {
    let count = Literal::usize_unsuffixed(content.items.len());

    let item_arms = content.items.iter().enumerate().map(|(index, item)| {
        let idx = Literal::u16_unsuffixed(index as u16);
        let info = item_info(item);
        quote!(#idx => #info)
    });

    let for_material_arms = content
        .item_for_material
        .iter()
        .enumerate()
        .map(|(index, item_id)| {
            let idx = Literal::u16_unsuffixed(index as u16);
            let id = Literal::u16_suffixed(*item_id);
            quote!(#idx => crate::item::ItemId(#id))
        });

    let id_of_arms = content
        .items
        .iter()
        .enumerate()
        .skip(1)
        .map(|(index, item)| {
            let name = &item.name;
            let id = Literal::u16_suffixed(index as u16);
            quote!(#name => Some(crate::item::ItemId(#id)))
        });

    let recipes = content.recipes.iter().map(recipe_tokens);

    quote! {
        pub const ITEM_COUNT: usize = #count;

        #[inline]
        pub fn item(id: crate::item::ItemId) -> &'static crate::item::ItemInfo {
            match id.0 {
                #(#item_arms,)*
                _ => unreachable!(),
            }
        }

        #[inline]
        pub fn item_for_material(id: crate::material::MaterialId) -> crate::item::ItemId {
            match id.0 {
                #(#for_material_arms,)*
                _ => unreachable!(),
            }
        }

        pub fn item_id_of(name: &str) -> Option<crate::item::ItemId> {
            match name {
                #(#id_of_arms,)*
                _ => None,
            }
        }

        pub const RECIPES: &[crate::item::Recipe] = &[#(#recipes),*];
    }
}

fn item_info(item: &ItemOut) -> TokenStream {
    let name = &item.name;
    let display = &item.display;
    let stack_max = Literal::u32_suffixed(item.stack_max);
    let sprite = &item.sprite;
    let place = match item.place {
        Some(id) => {
            let id = material_id(id);
            quote!(Some(#id))
        }
        None => quote!(None),
    };
    let tool = match item.tool {
        Some((tier, speed)) => {
            let tier = Literal::u8_suffixed(tier);
            let speed = Literal::f32_suffixed(speed);
            quote!(Some(crate::item::ToolSpec { tier: #tier, speed: #speed }))
        }
        None => quote!(None),
    };
    quote! {
        &crate::item::ItemInfo {
            name: #name,
            display: #display,
            stack_max: #stack_max,
            sprite: #sprite,
            place: #place,
            tool: #tool,
        }
    }
}

fn recipe_tokens(recipe: &RecipeOut) -> TokenStream {
    let inputs = recipe.inputs.iter().map(|(id, count)| {
        let id = Literal::u16_suffixed(*id);
        let count = Literal::u32_suffixed(*count);
        quote!((crate::item::ItemId(#id), #count))
    });
    let out_id = Literal::u16_suffixed(recipe.output.0);
    let out_count = Literal::u32_suffixed(recipe.output.1);
    quote! {
        crate::item::Recipe {
            inputs: &[#(#inputs),*],
            output: (crate::item::ItemId(#out_id), #out_count),
        }
    }
}
