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
        let open_flame = match &mat.ember {
            Some(ember) => matches!(ember.kind, fallingsand_material::EmberKind::Flame),
            None => true,
        };
        let ember = match &mat.ember {
            Some(ember) => {
                let burn = Literal::u64_suffixed(ember.burn);
                let emit = Literal::u64_suffixed(ember.emit);
                let residue = option_tokens(ember.residue.map(|(threshold, id)| {
                    let threshold = Literal::u64_suffixed(threshold);
                    let id = material_id(id);
                    quote!((#threshold, #id))
                }));
                let burnout = material_id(ember.burnout);
                let kind = match ember.kind {
                    fallingsand_material::EmberKind::Flame => {
                        quote!(crate::material::EmberKind::Flame)
                    }
                    fallingsand_material::EmberKind::Fuel => {
                        quote!(crate::material::EmberKind::Fuel)
                    }
                };
                quote! {
                    Some(crate::material::Ember {
                        burn: #burn,
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
                const OPEN_FLAME: bool = #open_flame;
                const EMBER: Option<crate::material::Ember> = #ember;
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

fn dynamics_tokens(dynamics: &fallingsand_material::Dynamics) -> TokenStream {
    use fallingsand_material::Dynamics;
    match dynamics {
        Dynamics::None => quote!(crate::material::Dynamics::None),
        Dynamics::Powder(d) => {
            let drag_keep = Literal::u32_suffixed(d.drag_keep_q16);
            let drag_keep_submerged = Literal::u32_suffixed(d.drag_keep_submerged_q16);
            let friction_keep = Literal::u32_suffixed(d.friction_keep_q16);
            let cohesion = Literal::u32_suffixed(d.cohesion_q16);
            let restitution = Literal::u32_suffixed(d.restitution_q16);
            let redirect_keep = Literal::u32_suffixed(d.redirect_keep_q16);
            let slide = Literal::u64_suffixed(d.slide_threshold);
            quote! {
                crate::material::Dynamics::Powder(crate::material::PowderDynamics {
                    drag_keep_q16: #drag_keep,
                    drag_keep_submerged_q16: #drag_keep_submerged,
                    friction_keep_q16: #friction_keep,
                    cohesion_q16: #cohesion,
                    restitution_q16: #restitution,
                    redirect_keep_q16: #redirect_keep,
                    slide_threshold: #slide,
                })
            }
        }
        Dynamics::Liquid(d) => {
            let drag_keep = Literal::u32_suffixed(d.drag_keep_q16);
            let drag_keep_submerged = Literal::u32_suffixed(d.drag_keep_submerged_q16);
            let friction_keep = Literal::u32_suffixed(d.friction_keep_q16);
            let cohesion = Literal::u32_suffixed(d.cohesion_q16);
            let restitution = Literal::u32_suffixed(d.restitution_q16);
            let redirect_keep = Literal::u32_suffixed(d.redirect_keep_q16);
            let flow = Literal::u64_suffixed(d.flow_threshold);
            quote! {
                crate::material::Dynamics::Liquid(crate::material::LiquidDynamics {
                    drag_keep_q16: #drag_keep,
                    drag_keep_submerged_q16: #drag_keep_submerged,
                    friction_keep_q16: #friction_keep,
                    cohesion_q16: #cohesion,
                    restitution_q16: #restitution,
                    redirect_keep_q16: #redirect_keep,
                    flow_threshold: #flow,
                })
            }
        }
        Dynamics::Gas(d) => {
            let drag_keep = Literal::u32_suffixed(d.drag_keep_q16);
            let cohesion = Literal::u32_suffixed(d.cohesion_q16);
            let restitution = Literal::u32_suffixed(d.restitution_q16);
            let redirect_keep = Literal::u32_suffixed(d.redirect_keep_q16);
            let turbulence = Literal::u32_suffixed(d.turbulence_q16);
            quote! {
                crate::material::Dynamics::Gas(crate::material::GasDynamics {
                    drag_keep_q16: #drag_keep,
                    cohesion_q16: #cohesion,
                    restitution_q16: #restitution,
                    redirect_keep_q16: #redirect_keep,
                    turbulence_q16: #turbulence,
                })
            }
        }
    }
}
