use fallingsand_core::{MaterialRegistry, REGION_SIZE_CELLS, RegionPos};
use fallingsand_worldgen::WorldGenerator;
use std::fs::File;
use std::io::BufWriter;
use std::time::Instant;

const MATERIALS_RON: &str = include_str!("../../../data/materials.ron");

struct Args {
    seed: u64,
    min: (i32, i32),
    max: (i32, i32),
    out: String,
    timing: bool,
}

fn parse_pair(text: &str) -> (i32, i32) {
    let (x, y) = text.split_once(',').expect("expected x,y");
    (x.parse().expect("x"), y.parse().expect("y"))
}

fn parse_args() -> Args {
    let mut args = Args {
        seed: 42,
        min: (-3, -2),
        max: (3, 1),
        out: "preview.png".into(),
        timing: false,
    };
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--seed" => args.seed = iter.next().expect("--seed value").parse().expect("seed"),
            "--min" => args.min = parse_pair(&iter.next().expect("--min value")),
            "--max" => args.max = parse_pair(&iter.next().expect("--max value")),
            "--out" => args.out = iter.next().expect("--out value"),
            "--timing" => args.timing = true,
            other => panic!("unknown argument {other:?}"),
        }
    }
    args
}

fn main() {
    let args = parse_args();
    let registry = MaterialRegistry::from_ron(MATERIALS_RON).expect("materials.ron is valid");
    let generator = WorldGenerator::new(args.seed, &registry).expect("generator");

    let size = REGION_SIZE_CELLS;
    let regions_x = (args.max.0 - args.min.0 + 1) as usize;
    let regions_y = (args.max.1 - args.min.1 + 1) as usize;
    let width = regions_x * size;
    let height = regions_y * size;
    let mut pixels = vec![0u8; width * height * 4];

    let mut total = std::time::Duration::ZERO;
    let mut slowest = std::time::Duration::ZERO;
    for region_y in args.min.1..=args.max.1 {
        for region_x in args.min.0..=args.max.0 {
            let started = Instant::now();
            let region = generator.generate_region(RegionPos::new(region_x, region_y));
            let elapsed = started.elapsed();
            total += elapsed;
            slowest = slowest.max(elapsed);
            if args.timing {
                println!("region ({region_x}, {region_y}): {elapsed:.2?}");
            }

            let origin_x = (region_x - args.min.0) as usize * size;
            let origin_y = (args.max.1 - region_y) as usize * size;
            for (chunk_index, chunk) in region.chunks().iter().enumerate() {
                let chunk_x = (chunk_index % 8) * 64;
                let chunk_y = (chunk_index / 8) * 64;
                for (cell_index, cell) in chunk.cells().iter().enumerate() {
                    let cell_x = cell_index % 64;
                    let cell_y = cell_index / 64;
                    let material = registry.get(cell.material);
                    let color = material.colors[cell.shade() as usize % material.colors.len()];
                    let px = origin_x + chunk_x + cell_x;
                    let py = origin_y + (size - 1 - (chunk_y + cell_y));
                    let index = (py * width + px) * 4;
                    if cell.is_air() {
                        pixels[index..index + 4].copy_from_slice(&[24, 26, 38, 255]);
                    } else {
                        pixels[index..index + 3].copy_from_slice(&color[..3]);
                        pixels[index + 3] = 255;
                    }
                }
            }
        }
    }

    let count = (regions_x * regions_y) as u32;
    println!(
        "generated {count} regions in {total:.2?} (avg {:.2?}, worst {slowest:.2?})",
        total / count,
    );

    let file = File::create(&args.out).expect("create output file");
    let mut encoder = png::Encoder::new(BufWriter::new(file), width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(&pixels).expect("png data");
    println!("wrote {}x{height} preview to {}", width, args.out);
}
