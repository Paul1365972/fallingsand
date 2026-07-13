//! Regenerate item/material icons from compiled content:
//! `cargo run -p fallingsand_core --example gen_icons`
//!
//! Material tiles are derived from each material's palette; the handful of crafted items (sticks, ingots, pickaxes) are drawn procedurally.

use fallingsand_core::content;
use image::{Rgba, RgbaImage};
use std::path::{Path, PathBuf};

const SIZE: u32 = 16;

fn shade(color: [u8; 3], factor: f64) -> Rgba<u8> {
    let channel = |c: u8| (c as f64 * factor).min(255.0) as u8;
    Rgba([channel(color[0]), channel(color[1]), channel(color[2]), 255])
}

fn blank() -> RgbaImage {
    RgbaImage::new(SIZE, SIZE)
}

fn put(img: &mut RgbaImage, x: i32, y: i32, color: Rgba<u8>) {
    if (0..SIZE as i32).contains(&x) && (0..SIZE as i32).contains(&y) {
        img.put_pixel(x as u32, y as u32, color);
    }
}

fn material_tile(colors: &[[u8; 4]]) -> RgbaImage {
    let mut img = blank();
    let n = colors.len() as i32;
    for y in 0..SIZE as i32 {
        for x in 0..SIZE as i32 {
            let h = (x * 7 + y * 131 + x * y * 17) & 0xFF;
            let [r, g, b, _] = colors[(h % n) as usize];
            let color = if x == 0 || x == 15 || y == 0 || y == 15 {
                shade([r, g, b], 0.55)
            } else if x == 1 || y == 1 {
                shade([r, g, b], 1.18)
            } else {
                Rgba([r, g, b, 255])
            };
            img.put_pixel(x as u32, y as u32, color);
        }
    }
    img
}

fn stick() -> RgbaImage {
    let mut img = blank();
    let wood = [140, 96, 54];
    for t in 0..10 {
        let (x, y) = (3 + t, 13 - t);
        put(&mut img, x, y - 1, shade(wood, 1.28));
        put(&mut img, x, y, shade(wood, 1.0));
        put(&mut img, x, y + 1, shade(wood, 0.6));
    }
    put(&mut img, 3, 13, shade(wood, 0.6));
    put(&mut img, 12, 3, shade(wood, 1.28));
    img
}

fn ingot(base: [u8; 3]) -> RgbaImage {
    let mut img = blank();
    for (y, a, b) in [(5, 6, 9), (6, 5, 10), (7, 4, 11), (8, 4, 11), (9, 4, 11)] {
        for x in a..=b {
            let color = if y == 5 {
                shade(base, 1.3)
            } else if y == 9 {
                shade(base, 0.62)
            } else if x == a || x == b {
                shade(base, 0.5)
            } else {
                shade(base, 1.0)
            };
            put(&mut img, x, y, color);
        }
    }
    put(&mut img, 7, 6, Rgba([255, 255, 255, 255]));
    put(&mut img, 8, 6, shade(base, 1.45));
    img
}

fn pickaxe(head: [u8; 3]) -> RgbaImage {
    let mut img = blank();
    let handle = [120, 80, 45];
    for (x, y) in [
        (8, 6),
        (8, 7),
        (9, 8),
        (9, 9),
        (9, 10),
        (10, 11),
        (10, 12),
        (10, 13),
    ] {
        put(&mut img, x, y, Rgba([handle[0], handle[1], handle[2], 255]));
        put(&mut img, x + 1, y, shade(handle, 0.62));
    }
    for (x, y) in [
        (3, 6),
        (4, 5),
        (5, 4),
        (6, 4),
        (7, 4),
        (8, 4),
        (9, 4),
        (10, 4),
        (11, 5),
        (12, 6),
        (13, 7),
    ] {
        put(&mut img, x, y - 1, shade(head, 1.3));
        put(&mut img, x, y, shade(head, 1.0));
        put(&mut img, x, y + 1, shade(head, 0.58));
    }
    img
}

fn missing() -> RgbaImage {
    let mut img = blank();
    for y in 0..SIZE as i32 {
        for x in 0..SIZE as i32 {
            let on = ((x / 4) + (y / 4)) % 2 == 0;
            let color = if on {
                Rgba([214, 0, 214, 255])
            } else {
                Rgba([24, 24, 24, 255])
            };
            img.put_pixel(x as u32, y as u32, color);
        }
    }
    img
}

fn crafted(name: &str) -> Option<RgbaImage> {
    Some(match name {
        "stick" => stick(),
        "iron_ingot" => ingot([190, 198, 210]),
        "gold_ingot" => ingot([236, 190, 48]),
        "wooden_pickaxe" => pickaxe([154, 105, 62]),
        "stone_pickaxe" => pickaxe([132, 134, 142]),
        "iron_pickaxe" => pickaxe([190, 198, 210]),
        _ => return None,
    })
}

fn save(img: &RgbaImage, path: &Path) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create icon directory");
    }
    img.save(path).expect("write icon png");
}

fn main() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/items");

    let mut materials = 0;
    let mut items = 0;
    for (_, info) in content::items() {
        if info.sprite.is_empty() {
            continue;
        }
        let path = out.join(format!("{}.png", info.sprite));
        if let Some(material) = info.place {
            save(&material_tile(content::material(material).colors), &path);
            materials += 1;
        } else if let Some(icon) = crafted(info.name) {
            save(&icon, &path);
            items += 1;
        } else {
            eprintln!("warning: no icon recipe for item `{}`", info.name);
        }
    }
    save(&missing(), &out.join("missing.png"));

    println!("materials: {materials}, items: {items}, plus missing");
}
