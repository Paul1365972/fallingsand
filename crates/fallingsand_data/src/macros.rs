macro_rules! materials {
    ( base: $base:expr; $( $name:ident = Material { $( $body:tt )* } ),* $(,)? ) => {
        pub(crate) const DEFS: &[fallingsand_core::Material] = &[
            $( fallingsand_core::Material {
                $( $body )*
                name: stringify!($name),
                ..fallingsand_core::Material::DEFAULT
            } ),*
        ];

        #[allow(dead_code)]
        pub(crate) const COUNT: u16 = {
            let mut n = 0u16;
            $( { let _ = stringify!($name); n += 1; } )*
            n
        };

        materials!(@handles $base, 0u16; $( $name, )*);
    };

    (@handles $base:expr, $idx:expr; ) => {};
    (@handles $base:expr, $idx:expr; $name:ident, $( $rest:ident, )* ) => {
        pub const $name: fallingsand_core::MaterialId =
            fallingsand_core::MaterialId($base + $idx);
        materials!(@handles $base, $idx + 1u16; $( $rest, )*);
    };
}

macro_rules! domains {
    ( $( $domain:ident ),* $(,)? ) => {
        $( mod $domain; )*

        #[allow(non_upper_case_globals)]
        pub(crate) mod base {
            domains!(@bases 0u16; $( $domain, )*);
        }

        $( pub use $domain::*; )*

        pub(crate) fn assemble() -> Vec<fallingsand_core::Material> {
            let mut all = Vec::new();
            $( all.extend_from_slice($domain::DEFS); )*
            all
        }
    };

    (@bases $acc:expr; ) => {};
    (@bases $acc:expr; $domain:ident, $( $rest:ident, )* ) => {
        pub const $domain: u16 = $acc;
        domains!(@bases $acc + super::$domain::COUNT; $( $rest, )*);
    };
}

macro_rules! reactions {
    ( $( $a:tt + $b:tt => $becomes_a:tt + $becomes_b:tt @ $rate:literal );* $(;)? ) => {
        pub(crate) fn reactions() -> Vec<fallingsand_core::ReactionDef> {
            vec![
                $( fallingsand_core::ReactionDef {
                    a: reactions!(@operand $a),
                    b: reactions!(@operand $b),
                    a_becomes: $becomes_a,
                    b_becomes: $becomes_b,
                    rate: $rate,
                } ),*
            ]
        }
    };
    (@operand [$tag:expr]) => { fallingsand_core::Operand::Tag($tag) };
    (@operand $material:expr) => { fallingsand_core::Operand::Material($material) };
}

macro_rules! items {
    ( $( $name:ident $display:literal $stack:literal $icon:expr );* $(;)? ) => {
        pub(crate) const ENTRIES: &[fallingsand_core::ItemEntry] = &[
            $( fallingsand_core::ItemEntry {
                name: stringify!($name),
                display: $display,
                stack_max: $stack,
                icon: fallingsand_core::IconSpec::MaterialSwatch($icon),
            } ),*
        ];

        pub mod item {
            items!(@handles 0u16; $( $name, )*);
        }
    };

    (@handles $idx:expr; ) => {};
    (@handles $idx:expr; $name:ident, $( $rest:ident, )* ) => {
        pub const $name: fallingsand_core::ItemId = fallingsand_core::ItemId(1 + $idx);
        items!(@handles $idx + 1u16; $( $rest, )*);
    };
}

macro_rules! recipes {
    (
        $(
            $first_count:literal $first:path $(, $more_count:literal $more:path)*
            => $output_count:literal $output:path
        );* $(;)?
    ) => {
        pub(crate) fn recipes(
            registry: &fallingsand_core::ItemRegistry,
        ) -> fallingsand_core::RecipeRegistry {
            fallingsand_core::RecipeRegistry::new(vec![
                $( fallingsand_core::Recipe {
                    inputs: vec![
                        (crate::recipes::resolve($first, registry), $first_count)
                        $(, (crate::recipes::resolve($more, registry), $more_count) )*
                    ],
                    output: (crate::recipes::resolve($output, registry), $output_count),
                } ),*
            ])
        }
    };
}
