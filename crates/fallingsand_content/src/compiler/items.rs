use super::{Catalog, Error, HashMap, ItemOut, Mat, MaterialId, RecipeOut, Tag, fail};
use crate::IngredientDef;

const MATERIAL_STACK_MAX: u32 = 10_000;

pub(super) fn build_items(
    catalog: &Catalog,
    materials: &[Mat],
    fuel_base: &[Option<MaterialId>],
) -> Result<(Vec<ItemOut>, Vec<u16>), Error> {
    let mut items = vec![ItemOut {
        name: "none".to_owned(),
        display: "None".to_owned(),
        stack_max: 0,
        sprite: String::new(),
        place: None,
        tool: None,
    }];

    let mut by_key: HashMap<String, u16> = HashMap::new();
    for (key, def) in &catalog.items {
        validate_ident("item key", key.as_str())?;
        let name = key.as_str().to_ascii_lowercase();
        let id = items.len() as u16;
        if by_key.insert(key.as_str().to_owned(), id).is_some() {
            return Err(fail(format!("duplicate item key `{key}`")));
        }
        items.push(ItemOut {
            display: def.display.clone(),
            stack_max: def.stack.max(1),
            sprite: name.clone(),
            place: None,
            tool: def.tool,
            name,
        });
    }

    let mut material_item = vec![0u16; materials.len()];
    for (index, mat) in materials.iter().enumerate().skip(1) {
        if mat.tags.contains(Tag::Player) {
            continue;
        }
        let id = items.len() as u16;
        items.push(ItemOut {
            name: format!("mat:{}", mat.name),
            display: pretty_name(&mat.name),
            stack_max: MATERIAL_STACK_MAX,
            sprite: format!("materials/{}", mat.name),
            place: Some(MaterialId(index as u16)),
            tool: None,
        });
        material_item[index] = id;
    }

    if items.len() > u16::MAX as usize {
        return Err(fail(format!("too many items: {}", items.len())));
    }

    let item_for_material = (0..materials.len())
        .map(|index| {
            let source = fuel_base[index].map_or(index, |base| base.0 as usize);
            material_item[source]
        })
        .collect();

    Ok((items, item_for_material))
}

pub(super) fn build_recipes(
    catalog: &Catalog,
    by_name: &HashMap<String, MaterialId>,
    item_for_material: &[u16],
) -> Result<Vec<RecipeOut>, Error> {
    let by_key: HashMap<&str, u16> = catalog
        .items
        .iter()
        .enumerate()
        .map(|(index, (key, _))| (key.as_str(), index as u16 + 1))
        .collect();

    let resolve = |ingredient: &IngredientDef| -> Result<u16, Error> {
        match ingredient {
            IngredientDef::Item(key) => by_key
                .get(key.as_str())
                .copied()
                .ok_or_else(|| fail(format!("recipes: unknown item `{key}`"))),
            IngredientDef::Material(key) => {
                let mat = by_name
                    .get(key.as_str())
                    .ok_or_else(|| fail(format!("recipes: unknown material `{key}`")))?;
                let item = item_for_material[mat.0 as usize];
                if item == 0 {
                    Err(fail(format!("recipes: material `{key}` has no item form")))
                } else {
                    Ok(item)
                }
            }
        }
    };

    let mut recipes = Vec::with_capacity(catalog.recipes.len());
    for def in &catalog.recipes {
        let mut inputs = Vec::with_capacity(def.inputs.len());
        for (ingredient, count) in &def.inputs {
            inputs.push((resolve(ingredient)?, *count));
        }
        let output = (resolve(&def.output.0)?, def.output.1);
        recipes.push(RecipeOut { inputs, output });
    }
    Ok(recipes)
}

pub(super) fn validate_ident(kind: &str, name: &str) -> Result<(), Error> {
    if name.is_empty()
        || !name.as_bytes()[0].is_ascii_uppercase()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(fail(format!(
            "{kind} `{name}` must be an UPPER_SNAKE_CASE Rust identifier"
        )));
    }
    Ok(())
}

fn pretty_name(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for (index, word) in raw.split('_').enumerate() {
        if index > 0 {
            out.push(' ');
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}
