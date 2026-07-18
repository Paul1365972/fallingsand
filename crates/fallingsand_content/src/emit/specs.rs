use super::{material_id, phase_path};
use crate::model::Content;
use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::quote;

pub fn emit(content: &Content) -> TokenStream {
    let len = content.materials.len();
    let impls = content.materials.iter().enumerate().map(|(index, mat)| {
        let spec = Ident::new(&mat.spec_name, Span::call_site());
        let phase = phase_path(mat.phase);
        let density_milli = Literal::i32_suffixed(mat.density_milli);
        let is_hot = mat.tags.contains(fallingsand_material::Tag::Hot);
        let ignition = option_tokens(content.ignitions[index].as_ref().map(|ignition| {
            let into = material_id(ignition.into);
            let open = Literal::u64_suffixed(ignition.open);
            let sealed = Literal::u64_suffixed(ignition.sealed);
            quote! {
                crate::material::Ignition {
                    into: #into,
                    open: #open,
                    sealed: #sealed,
                }
            }
        }));
        let burning = match &mat.burning {
            Some(burning) => {
                let burn = Literal::u64_suffixed(burning.burn);
                let sealed = match burning.sealed {
                    fallingsand_material::SealedBurn::Becomes(id) => {
                        let id = material_id(id);
                        quote!(crate::material::SealedBurn::Becomes(#id))
                    }
                    fallingsand_material::SealedBurn::Smoulder(threshold) => {
                        let threshold = Literal::u64_suffixed(threshold);
                        quote!(crate::material::SealedBurn::Smoulder(#threshold))
                    }
                };
                let emit = Literal::u64_suffixed(burning.emit);
                let residue = option_tokens(burning.residue.map(|(threshold, id)| {
                    let threshold = Literal::u64_suffixed(threshold);
                    let id = material_id(id);
                    quote!((#threshold, #id))
                }));
                let burnout = material_id(burning.burnout);
                let kind = match burning.kind {
                    fallingsand_material::BurningKind::Flame => {
                        quote!(crate::material::BurningKind::Flame)
                    }
                    fallingsand_material::BurningKind::Fuel => {
                        quote!(crate::material::BurningKind::Fuel)
                    }
                };
                quote! {
                    Some(crate::material::Burning {
                        burn: #burn,
                        sealed: #sealed,
                        emit: #emit,
                        residue: #residue,
                        burnout: #burnout,
                        kind: #kind,
                    })
                }
            }
            None => quote!(None),
        };
        let decay = option_tokens(mat.decay.map(|(threshold, id)| {
            let threshold = Literal::u64_suffixed(threshold);
            let id = material_id(id);
            quote!((#threshold, #id))
        }));
        let is_reactive = mat.reactive;
        let dynamics = dynamics_tokens(&mat.dynamics);
        let row = &content.reactions[index * len..(index + 1) * len];
        let reactions = if row.iter().all(Option::is_none) {
            quote!(&crate::content::EMPTY_ROW)
        } else {
            let row_name = Ident::new(&format!("ROW_{index}"), Span::call_site());
            quote!(&crate::content::#row_name)
        };
        quote! {
            pub struct #spec;

            impl crate::content::spec::MatSpec for #spec {
                const PHASE: crate::material::Phase = #phase;
                const DENSITY_MILLI: i32 = #density_milli;
                const IS_HOT: bool = #is_hot;
                const IGNITION: Option<crate::material::Ignition> = #ignition;
                const BURNING: Option<crate::material::Burning> = #burning;
                const DECAY: Option<(u64, crate::material::MaterialId)> = #decay;
                const IS_REACTIVE: bool = #is_reactive;
                const DYNAMICS: crate::material::Dynamics = #dynamics;
                const REACTIONS:
                    &'static [crate::material::Reaction; crate::content::MATERIAL_COUNT] =
                    #reactions;
            }
        }
    });
    let entries = content.materials.iter().enumerate().map(|(index, mat)| {
        let idx = Literal::u16_suffixed(index as u16);
        let const_name = Ident::new(&mat.const_name, Span::call_site());
        let spec_name = Ident::new(&mat.spec_name, Span::call_site());
        quote!((#idx, #const_name, #spec_name))
    });

    quote! {
        pub mod specs {
            #(#impls)*
        }

        #[macro_export]
        macro_rules! for_each_material {
            ($cb:ident) => {
                $cb! { #(#entries),* }
            };
        }
    }
}

fn option_tokens(value: Option<TokenStream>) -> TokenStream {
    match value {
        Some(inner) => quote!(Some(#inner)),
        None => quote!(None),
    }
}

fn velocity_factor_tokens(factor: fallingsand_material::VelocityFactor) -> TokenStream {
    let raw = Literal::u32_suffixed(factor.raw());
    quote!(crate::material::VelocityFactor::from_raw(#raw))
}

fn dynamics_tokens(dynamics: &fallingsand_material::Dynamics) -> TokenStream {
    use fallingsand_material::Dynamics;
    match dynamics {
        Dynamics::None => quote!(crate::material::Dynamics::None),
        Dynamics::Powder(d) => {
            let drag_keep = velocity_factor_tokens(d.air_drag_keep);
            let drag_keep_submerged = velocity_factor_tokens(d.submerged_drag_keep);
            let friction_keep = velocity_factor_tokens(d.ground_friction_keep);
            let restitution = velocity_factor_tokens(d.restitution);
            let redirect_keep = velocity_factor_tokens(d.deflect_keep);
            let slide_start = Literal::u64_suffixed(d.topple_start_threshold);
            let slide_keep = Literal::u64_suffixed(d.topple_keep_threshold);
            quote! {
                crate::material::Dynamics::Powder(crate::material::PowderDynamics {
                    air_drag_keep: #drag_keep,
                    submerged_drag_keep: #drag_keep_submerged,
                    ground_friction_keep: #friction_keep,
                    restitution: #restitution,
                    deflect_keep: #redirect_keep,
                    topple_start_threshold: #slide_start,
                    topple_keep_threshold: #slide_keep,
                })
            }
        }
        Dynamics::Liquid(d) => {
            let drag_keep = velocity_factor_tokens(d.air_drag_keep);
            let drag_keep_submerged = velocity_factor_tokens(d.submerged_drag_keep);
            let friction_keep = velocity_factor_tokens(d.ground_friction_keep);
            let cohesion = velocity_factor_tokens(d.cohesion);
            let restitution = velocity_factor_tokens(d.restitution);
            let redirect_keep = velocity_factor_tokens(d.deflect_keep);
            let flow = Literal::u64_suffixed(d.flow_threshold);
            quote! {
                crate::material::Dynamics::Liquid(crate::material::LiquidDynamics {
                    air_drag_keep: #drag_keep,
                    submerged_drag_keep: #drag_keep_submerged,
                    ground_friction_keep: #friction_keep,
                    cohesion: #cohesion,
                    restitution: #restitution,
                    deflect_keep: #redirect_keep,
                    flow_threshold: #flow,
                })
            }
        }
        Dynamics::Gas(d) => {
            let drag_keep = velocity_factor_tokens(d.air_drag_keep);
            let cohesion = velocity_factor_tokens(d.cohesion);
            let restitution = velocity_factor_tokens(d.restitution);
            let redirect_keep = velocity_factor_tokens(d.deflect_keep);
            let turbulence_q16 = Literal::u32_suffixed(d.turbulence_q16);
            quote! {
                crate::material::Dynamics::Gas(crate::material::GasDynamics {
                    air_drag_keep: #drag_keep,
                    cohesion: #cohesion,
                    restitution: #restitution,
                    deflect_keep: #redirect_keep,
                    turbulence_q16: #turbulence_q16,
                })
            }
        }
    }
}
