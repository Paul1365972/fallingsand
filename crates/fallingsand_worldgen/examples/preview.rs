use fallingsand_core::{CHUNK_SIZE, CellOffset, ChunkOffset, REGION_SIZE_CELLS, RegionPos};
use fallingsand_worldgen::WorldGenerator;
use std::fs::File;
use std::io::BufWriter;
use std::time::Instant;

struct Args {
    seed: u64,
    min: (i32, i32),
    max: (i32, i32),
    step: usize,
    out: String,
}

fn parse_pair(text: &str) -> (i32, i32) {
    let (x, y) = text.split_once(',').expect("expected x,y");
    (x.parse().expect("x"), y.parse().expect("y"))
}

fn parse_args() -> Args {
    let mut args = Args {
        seed: 42,
        min: (-1, -1),
        max: (0, 0),
        step: 1,
        out: "preview.png".into(),
    };
    let mut iter = std::env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--seed" => args.seed = iter.next().expect("--seed value").parse().expect("seed"),
            "--min" => args.min = parse_pair(&iter.next().expect("--min value")),
            "--max" => args.max = parse_pair(&iter.next().expect("--max value")),
            "--step" => args.step = iter.next().expect("--step value").parse().expect("step"),
            "--out" => args.out = iter.next().expect("--out value"),
            other => panic!("unknown argument {other:?}"),
        }
    }
    args.step = args.step.max(1);
    args
}

const DEPTHS: [i32; 9] = [-60, -180, -420, -900, -1800, -3600, -7000, -13000, -26000];

fn cell_at(generator: &WorldGenerator, x: i32, y: i32) -> &'static str {
    let region = generator.generate_region(RegionPos::new(
        x.div_euclid(REGION_SIZE_CELLS as i32),
        y.div_euclid(REGION_SIZE_CELLS as i32),
    ));
    let local_x = x.rem_euclid(REGION_SIZE_CELLS as i32) as usize;
    let local_y = y.rem_euclid(REGION_SIZE_CELLS as i32) as usize;
    let chunk = ChunkOffset::new((local_x / CHUNK_SIZE) as u8, (local_y / CHUNK_SIZE) as u8);
    let cell = region.chunks()[chunk.index()].cells()
        [CellOffset::new((local_x % CHUNK_SIZE) as u8, (local_y % CHUNK_SIZE) as u8).index()];
    fallingsand_core::content::material(cell.material).name
}

fn probe(generator: &WorldGenerator, x: i32) {
    println!("--- column x={x} surface={}", generator.surface_height(x));
    for y in DEPTHS {
        let (realm, domain) = generator.location_names(x, y);
        println!(
            "  y={y:>7} {realm:<16} {domain:<20} {}",
            cell_at(generator, x, y)
        );
    }
}

fn histogram(title: &str, counts: &[(&'static str, usize)], total: usize) {
    let mut sorted = counts.to_vec();
    sorted.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    println!("{title} ({total} samples)");
    for (name, count) in sorted {
        println!("  {:>5.1}%  {name}", count as f64 * 100.0 / total as f64);
    }
}

fn tally(counts: &mut Vec<(&'static str, usize)>, name: &'static str) {
    match counts.iter_mut().find(|(existing, _)| *existing == name) {
        Some(entry) => entry.1 += 1,
        None => counts.push((name, 1)),
    }
}

const MARKERS: [&str; 6] = ["beam", "planks", "rope", "torch", "lumen_lamp", "gunpowder"];

fn structures(generator: &WorldGenerator, region_y: i32) {
    let mut counts = [0usize; MARKERS.len()];
    let mut present = [0usize; MARKERS.len()];
    let mut regions = 0;
    let mut with_any = 0;
    for region_x in -12..=12 {
        let region = generator.generate_region(RegionPos::new(region_x, region_y));
        regions += 1;
        let mut here = [0usize; MARKERS.len()];
        for chunk in region.chunks() {
            for cell in chunk.cells() {
                let name = fallingsand_core::content::material(cell.material).name;
                if let Some(index) = MARKERS.iter().position(|marker| *marker == name) {
                    here[index] += 1;
                }
            }
        }
        let mut any = false;
        for (index, count) in here.iter().enumerate() {
            counts[index] += count;
            if *count > 0 {
                present[index] += 1;
                any = true;
            }
        }
        if any {
            with_any += 1;
        }
    }
    let total: usize = counts.iter().sum();
    print!(
        "structures at region y={region_y}: {total} cells, {with_any}/{regions} regions populated"
    );
    for ((marker, count), seen) in MARKERS.iter().zip(counts).zip(present) {
        if count > 0 {
            print!("  {marker} {count} in {seen}");
        }
    }
    println!();
}

fn air_fraction(generator: &WorldGenerator, region_y: i32) -> f64 {
    let mut air = 0usize;
    let mut total = 0usize;
    for region_x in -3..=3 {
        let region = generator.generate_region(RegionPos::new(region_x * 5, region_y));
        for chunk in region.chunks() {
            for cell in chunk.cells() {
                total += 1;
                if cell.is_air() {
                    air += 1;
                }
            }
        }
    }
    air as f64 * 100.0 / total as f64
}

fn seams(generator: &WorldGenerator) {
    let mut straight = 0;
    let mut ragged = 0;
    let mut worst = 0;
    for base_x in -20_000..20_000 {
        let probe_y = -250;
        let here = generator.location_names(base_x, probe_y).1;
        let next = generator.location_names(base_x + 1, probe_y).1;
        if here == next {
            continue;
        }
        let mut span = 0;
        for step in 1..=120 {
            let y = probe_y - step * 2;
            let a = generator.location_names(base_x, y).1;
            let b = generator.location_names(base_x + 1, y).1;
            if a == here && b == next {
                span = step;
            } else {
                break;
            }
        }
        if span >= 20 {
            straight += 1;
            worst = worst.max(span * 2);
        } else {
            ragged += 1;
        }
    }
    println!(
        "sub-biome seams at y=-250: {straight} straight (>=40 cells of identical vertical edge), {ragged} ragged, longest {worst} cells"
    );

    const DAYLIGHT: [&str; 8] = [
        "Meadowveldt",
        "Pinewood",
        "Snowfield",
        "Dunesea",
        "Sunken Coast",
        "Saltmarsh",
        "Ashland",
        "Sporehall",
    ];
    let mut bare = 0;
    let mut total = 0;
    for x in -40_000..40_000 {
        let surface = generator.surface_height(x);
        let biome = generator.location_names(x, surface - 1).0;
        total += 1;
        if !DAYLIGHT.contains(&biome) {
            bare += 1;
        }
    }
    println!(
        "surface without a daylight biome: {:.1}% of {total} columns",
        bare as f64 * 100.0 / total.max(1) as f64
    );
}

fn main() {
    let args = parse_args();
    let generator = WorldGenerator::new(args.seed);
    if std::env::var("PROBE").is_ok() {
        let mut min = i32::MAX;
        let mut max = i32::MIN;
        let mut total = 0i64;
        let mut below_sea = 0;
        let span = 200_000;
        let mut samples = 0i64;
        for x in (-span..span).step_by(97) {
            let height = generator.surface_height(x);
            min = min.min(height);
            max = max.max(height);
            total += height as i64;
            samples += 1;
            if height < 0 {
                below_sea += 1;
            }
        }
        println!(
            "surface over +/-{span} cells: min {min} max {max} mean {} sea {:.1}%",
            total / samples,
            below_sea as f64 * 100.0 / samples as f64
        );

        let mut worst = 0;
        let mut worst_at = 0;
        let mut previous = generator.surface_height(-40_000);
        for x in -40_000..40_000 {
            let height = generator.surface_height(x);
            if (height - previous).abs() > worst {
                worst = (height - previous).abs();
                worst_at = x;
            }
            previous = height;
        }
        println!("steepest adjacent column jump: {worst} cells at x={worst_at}");
        let mut surface_counts = Vec::new();
        let mut surface_total = 0;
        for x in (-span..span).step_by(53) {
            let (_, domain) = generator.location_names(x, generator.surface_height(x) + 1);
            tally(&mut surface_counts, domain);
            surface_total += 1;
        }
        histogram("surface sub-biomes", &surface_counts, surface_total);

        for y in DEPTHS {
            let mut realms = Vec::new();
            let mut counts = Vec::new();
            let mut total = 0;
            for x in (-span..span).step_by(53) {
                let (realm, domain) = generator.location_names(x, y);
                tally(&mut realms, realm);
                tally(&mut counts, domain);
                total += 1;
            }
            histogram(&format!("y={y} biomes"), &realms, total);
            histogram(&format!("y={y} sub-biomes"), &counts, total);
        }

        for region_y in [-1, -2, -4, -8, -16, -32] {
            println!(
                "air at region y={region_y} (y~{}): {:.1}%",
                region_y * 512,
                air_fraction(&generator, region_y)
            );
        }

        for region_y in [-1, -2, -3, -4, -5, -6, -8] {
            structures(&generator, region_y);
        }

        seams(&generator);

        for x in [-7800, -3000, 0, 3000, 7000] {
            probe(&generator, x);
        }
        return;
    }

    let size = REGION_SIZE_CELLS;
    let step = args.step;
    let cells_x = (args.max.0 - args.min.0 + 1) as usize * size;
    let cells_y = (args.max.1 - args.min.1 + 1) as usize * size;
    let width = cells_x / step;
    let height = cells_y / step;
    let mut pixels = vec![0u8; width * height * 4];

    let start = Instant::now();
    let mut regions = 0usize;
    for region_y in args.min.1..=args.max.1 {
        for region_x in args.min.0..=args.max.0 {
            let region = generator.generate_region(RegionPos::new(region_x, region_y));
            regions += 1;
            let origin_x = (region_x - args.min.0) as usize * size;
            let origin_y = (args.max.1 - region_y) as usize * size;
            for (chunk_index, chunk) in region.chunks().iter().enumerate() {
                let chunk_off = ChunkOffset::from_index(chunk_index);
                let chunk_x = chunk_off.x as usize * CHUNK_SIZE;
                let chunk_y = chunk_off.y as usize * CHUNK_SIZE;
                for (cell_index, cell) in chunk.cells().iter().enumerate() {
                    let cell_off = CellOffset::from_index(cell_index);
                    let cell_x = origin_x + chunk_x + cell_off.x as usize;
                    let cell_y = origin_y + (size - 1 - (chunk_y + cell_off.y as usize));
                    if !cell_x.is_multiple_of(step) || !cell_y.is_multiple_of(step) {
                        continue;
                    }
                    let px = cell_x / step;
                    let py = cell_y / step;
                    if px >= width || py >= height {
                        continue;
                    }
                    let index = (py * width + px) * 4;
                    if cell.is_air() {
                        pixels[index..index + 4].copy_from_slice(&[24, 26, 38, 255]);
                    } else {
                        let material = fallingsand_core::content::material(cell.material);
                        let color = material.colors[cell.shade as usize % material.colors.len()];
                        pixels[index..index + 3].copy_from_slice(&color[..3]);
                        pixels[index + 3] = 255;
                    }
                }
            }
        }
    }
    let elapsed = start.elapsed();

    let file = File::create(&args.out).expect("create output file");
    let mut encoder = png::Encoder::new(BufWriter::new(file), width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(&pixels).expect("png data");
    println!(
        "wrote {width}x{height} ({cells_x}x{cells_y} cells, step {step}) to {}",
        args.out
    );
    println!(
        "generated {regions} regions in {:.2?} ({:.1} ms/region)",
        elapsed,
        elapsed.as_secs_f64() * 1000.0 / regions as f64
    );
}
