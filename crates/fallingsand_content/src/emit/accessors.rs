use super::{accessor_fn, colors_tokens, material_id, phase_path, tags_tokens};
use crate::compiler::{Content, mining_tier_from_hardness};
use fallingsand_material::{Dynamics, Reaction};
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
    let restitution = accessor_fn(
        "restitution",
        quote!(crate::material::Q16),
        content.materials.iter().map(|mat| {
            let raw = (f64::from(mat.restitution.clamp(0.0, 1.0)) * 65536.0).round() as u32;
            let value = Literal::u32_suffixed(raw);
            quote!(crate::material::Q16::from_raw(#value))
        }),
        true,
    );
    let friction = accessor_fn(
        "friction",
        quote!(crate::material::Q16),
        content.materials.iter().map(|mat| {
            let raw = (f64::from(mat.friction.clamp(0.0, 4.0)) * 65536.0).round() as u32;
            let value = Literal::u32_suffixed(raw);
            quote!(crate::material::Q16::from_raw(#value))
        }),
        true,
    );
    let bond_group = accessor_fn(
        "bond_group",
        quote!(u8),
        content.materials.iter().map(|mat| {
            let value = Literal::u8_suffixed(mat.bond_group);
            quote!(#value)
        }),
        true,
    );
    let repose_layers = accessor_fn(
        "repose_layers",
        quote!(u32),
        content.materials.iter().map(|mat| {
            let value = Literal::u32_suffixed(mat.repose_layers);
            quote!(#value)
        }),
        true,
    );
    let flow_threshold = accessor_fn(
        "flow_threshold",
        quote!(u64),
        content.materials.iter().map(|mat| {
            let value = Literal::u64_suffixed(mat.flow_threshold);
            quote!(#value)
        }),
        true,
    );
    let liquid_impact = accessor_fn(
        "liquid_impact",
        quote!(crate::material::Q16),
        content.materials.iter().map(|mat| {
            let raw = match mat.dynamics {
                Dynamics::Liquid(d) => d.impact_keep.raw(),
                _ => 0,
            };
            let value = Literal::u32_suffixed(raw);
            quote!(crate::material::Q16::from_raw(#value))
        }),
        true,
    );
    let liquid_exchange_thresholds = content
        .liquid_exchange_thresholds
        .iter()
        .map(|&threshold| Literal::u64_suffixed(threshold))
        .collect::<Vec<_>>();
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
            let entity_grip = Literal::f32_suffixed(mat.entity_grip);
            let entity_bounce = Literal::f32_suffixed(mat.entity_bounce);
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
                    entity_grip: #entity_grip,
                    entity_bounce: #entity_bounce,
                    contact_damage: #contact_damage,
                    emission: [#emission_r, #emission_g, #emission_b],
                    flicker: #flicker,
                }
            }
        }),
        false,
    );
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
        #restitution
        #friction
        #bond_group
        #repose_layers
        #flow_threshold
        #liquid_impact
        #ignition
        #material

        #[inline]
        pub const fn bonds(
            a: crate::material::MaterialId,
            b: crate::material::MaterialId,
        ) -> bool {
            bond_group(a) != u8::MAX && bond_group(a) == bond_group(b)
        }

        const LIQUID_EXCHANGE_THRESHOLDS: [u64; MATERIAL_COUNT * MATERIAL_COUNT] =
            [#(#liquid_exchange_thresholds),*];

        #[inline]
        pub const fn liquid_exchange_threshold(
            a: crate::material::MaterialId,
            b: crate::material::MaterialId,
        ) -> u64 {
            LIQUID_EXCHANGE_THRESHOLDS[a.0 as usize * MATERIAL_COUNT + b.0 as usize]
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
