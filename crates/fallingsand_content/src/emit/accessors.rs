use super::{accessor_fn, colors_tokens, material_id, phase_path, tags_tokens};
use crate::model::{Content, mining_tier_from_hardness};
use fallingsand_material::Reaction;
use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::quote;

pub fn emit(content: &Content) -> TokenStream {
    let len = content.materials.len();
    let count = Literal::usize_unsuffixed(len);
    let handles = content.materials.iter().enumerate().map(|(index, mat)| {
        let name = Ident::new(&mat.const_name, Span::call_site());
        let id = material_id(fallingsand_material::MaterialId(index as u16));
        quote!(pub const #name: crate::material::MaterialId = #id;)
    });
    let phase = accessor_fn(
        "phase",
        quote!(crate::material::Phase),
        content.materials.iter().map(|mat| phase_path(mat.phase)),
        true,
    );
    let density_milli = accessor_fn(
        "density_milli",
        quote!(i32),
        content.materials.iter().map(|mat| {
            let value = Literal::i32_suffixed(mat.density_milli);
            quote!(#value)
        }),
        true,
    );
    let tags = accessor_fn(
        "tags",
        quote!(crate::material::Tags),
        content.materials.iter().map(|mat| tags_tokens(mat.tags)),
        true,
    );
    let is_rigid_capable = accessor_fn(
        "is_rigid_capable",
        quote!(bool),
        content.materials.iter().map(|mat| {
            let value = mat.rigid_capable;
            quote!(#value)
        }),
        true,
    );
    let bond_group = accessor_fn(
        "bond_group",
        quote!(u8),
        content.materials.iter().map(|mat| {
            let value = mat.bond_group.unwrap_or(u8::MAX);
            quote!(#value)
        }),
        true,
    );
    let ignition = accessor_fn(
        "ignition",
        quote!(Option<crate::material::Ignition>),
        content.ignitions.iter().map(|slot| match slot {
            Some(ignition) => {
                let into = material_id(ignition.into);
                let open = Literal::u64_suffixed(ignition.open);
                let sealed = Literal::u64_suffixed(ignition.sealed);
                quote! {
                    Some(crate::material::Ignition {
                        into: #into,
                        open: #open,
                        sealed: #sealed,
                    })
                }
            }
            None => quote!(None),
        }),
        true,
    );
    let material = accessor_fn(
        "material",
        quote!(&'static crate::material::MaterialInfo),
        content.materials.iter().map(|mat| {
            let name = &mat.name;
            let colors = colors_tokens(&mat.colors);
            let hardness = Literal::f32_suffixed(mat.hardness);
            let mining_tier = Literal::u8_suffixed(mining_tier_from_hardness(mat.hardness));
            let restitution = Literal::f32_suffixed(mat.restitution);
            let surface_grip = Literal::f32_suffixed(mat.surface_grip);
            let surface_bounce = Literal::f32_suffixed(mat.surface_bounce);
            let contact_damage = Literal::f32_suffixed(mat.contact_damage);
            let [er, eg, eb] = mat.emission;
            let emission_r = Literal::f32_suffixed(er);
            let emission_g = Literal::f32_suffixed(eg);
            let emission_b = Literal::f32_suffixed(eb);
            let flicker = Literal::f32_suffixed(mat.flicker);
            quote! {
                &crate::material::MaterialInfo {
                    name: #name,
                    colors: #colors,
                    hardness: #hardness,
                    mining_tier: #mining_tier,
                    restitution: #restitution,
                    surface_grip: #surface_grip,
                    surface_bounce: #surface_bounce,
                    contact_damage: #contact_damage,
                    emission: [#emission_r, #emission_g, #emission_b],
                    flicker: #flicker,
                }
            }
        }),
        false,
    );
    let bond_group_count = Literal::usize_unsuffixed(content.bond_masks.len());
    let bond_masks = content
        .bond_masks
        .iter()
        .map(|&mask| Literal::u32_suffixed(mask))
        .collect::<Vec<_>>();
    let mut row_consts = Vec::new();
    for index in 0..len {
        let row = &content.reactions[index * len..(index + 1) * len];
        if row.iter().all(Option::is_none) {
            continue;
        }
        let row_name = Ident::new(&format!("ROW_{index}"), Span::call_site());
        let entries = row.iter().map(|slot| reaction_tokens(slot.as_ref()));
        row_consts.push(quote! {
            const #row_name: [crate::material::Reaction; MATERIAL_COUNT] = [#(#entries),*];
        });
    }

    quote! {
        pub const MATERIAL_COUNT: usize = #count;

        pub mod material {
            #(#handles)*
        }

        #phase
        #density_milli
        #tags
        #is_rigid_capable
        #bond_group
        #ignition
        #material

        const BOND_MASKS: [u32; #bond_group_count] = [#(#bond_masks),*];

        #[inline]
        pub const fn bonds(a: crate::material::MaterialId, b: crate::material::MaterialId) -> bool {
            let ga = bond_group(a);
            let gb = bond_group(b);
            ga != u8::MAX && gb != u8::MAX && BOND_MASKS[ga as usize] & (1 << gb) != 0
        }

        const NO_REACTION: crate::material::Reaction = crate::material::Reaction {
            becomes: crate::material::MaterialId(0u16),
            other_becomes: crate::material::MaterialId(0u16),
            threshold: 0u64,
        };
        const EMPTY_ROW: [crate::material::Reaction; MATERIAL_COUNT] =
            [NO_REACTION; MATERIAL_COUNT];
        #(#row_consts)*
    }
}

fn reaction_tokens(slot: Option<&Reaction>) -> TokenStream {
    match slot {
        Some(reaction) => {
            let becomes = material_id(reaction.becomes);
            let other_becomes = material_id(reaction.other_becomes);
            let threshold = Literal::u64_suffixed(reaction.threshold);
            quote! {
                crate::material::Reaction {
                    becomes: #becomes,
                    other_becomes: #other_becomes,
                    threshold: #threshold,
                }
            }
        }
        None => quote!(NO_REACTION),
    }
}
