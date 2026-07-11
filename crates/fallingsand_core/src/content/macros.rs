macro_rules! items {
    ( $( $name:ident $display:literal $stack:literal $icon:expr );* $(;)? ) => {
        pub(crate) const ENTRIES: &[crate::item::ItemEntry] = &[
            $( crate::item::ItemEntry {
                name: stringify!($name),
                display: $display,
                stack_max: $stack,
                icon: crate::item::IconSpec::MaterialSwatch($icon),
            } ),*
        ];

        pub mod item {
            crate::content::macros::items!(@handles 0u16; $( $name, )*);
        }
    };

    (@handles $idx:expr; ) => {};
    (@handles $idx:expr; $name:ident, $( $rest:ident, )* ) => {
        pub const $name: crate::item::ItemId = crate::item::ItemId(1 + $idx);
        crate::content::macros::items!(@handles $idx + 1u16; $( $rest, )*);
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
