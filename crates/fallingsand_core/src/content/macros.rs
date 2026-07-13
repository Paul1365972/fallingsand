macro_rules! items {
    (@munch [$($entries:tt)*] [$($handles:tt)*] $idx:expr; ) => {
        pub(crate) const ENTRIES: &[crate::item::ItemEntry] = &[$($entries)*];
        pub mod item {
            $($handles)*
        }
    };

    (@munch [$($entries:tt)*] [$($handles:tt)*] $idx:expr;
        $name:ident $display:literal $stack:literal
        $(tool(tier: $tier:literal, speed: $speed:literal))?
        ; $($rest:tt)*
    ) => {
        crate::content::macros::items!(@munch
            [$($entries)* crate::item::ItemEntry {
                name: stringify!($name),
                display: $display,
                stack_max: $stack,
                tool: crate::content::macros::items!(@tool $($tier, $speed)?),
            },]
            [$($handles)*
                pub const $name: crate::item::ItemId = crate::item::ItemId(1 + $idx);]
            $idx + 1u16; $($rest)*);
    };

    (@tool) => { None };
    (@tool $tier:literal, $speed:literal) => {
        Some(crate::item::ToolSpec { tier: $tier, speed: $speed })
    };

    ( $($body:tt)* ) => {
        crate::content::macros::items!(@munch [] [] 0u16; $($body)*);
    };
}
pub(crate) use items;

macro_rules! recipes {
    (
        $(
            $first_count:literal $first:path $(, $more_count:literal $more:path)*
            => $output_count:literal $output:path
        );* $(;)?
    ) => {
        pub(crate) fn recipes(
            registry: &crate::item::ItemRegistry,
        ) -> crate::item::RecipeRegistry {
            crate::item::RecipeRegistry::new(vec![
                $( crate::item::Recipe {
                    inputs: vec![
                        (crate::content::recipes::resolve($first, registry), $first_count)
                        $(, (crate::content::recipes::resolve($more, registry), $more_count) )*
                    ],
                    output: (crate::content::recipes::resolve($output, registry), $output_count),
                } ),*
            ])
        }
    };
}
pub(crate) use recipes;
