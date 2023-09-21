#![feature(test)]
#[cfg(test)]
extern crate test;

use test::{black_box, Bencher};

const SIZE: usize = 256;
type Grid = Vec<u64>;

#[bench]
fn linear(b: &mut Bencher) {
    let mut grid: Grid = vec![1; SIZE * SIZE];
    b.iter(|| {
        let mut super_sum = 0u64;

        for y in 1..(SIZE - 1) {
            for x in 1..(SIZE - 1) {
                let index = y * SIZE + x;
                let mut sum = 0u64;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let nx = x as isize + dx;
                        let ny = y as isize + dy;
                        let index = (ny as usize) * SIZE + (nx as usize);
                        sum = sum.wrapping_add(black_box(grid[index]));
                    }
                }
                grid[index] = sum;
                super_sum = super_sum.wrapping_add(sum);
            }
        }
        super_sum
    });
}

#[bench]
fn orthogonal(b: &mut Bencher) {
    let mut grid: Grid = vec![1; SIZE * SIZE];
    b.iter(|| {
        let mut super_sum = 0u64;

        for x in 1..(SIZE - 1) {
            for y in 1..(SIZE - 1) {
                let index = y * SIZE + x;
                let mut sum = 0u64;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let nx = x as isize + dx;
                        let ny = y as isize + dy;
                        let index = (ny as usize) * SIZE + (nx as usize);
                        sum = sum.wrapping_add(black_box(grid[index]));
                    }
                }
                grid[index] = sum;
                super_sum = super_sum.wrapping_add(sum);
            }
        }
        super_sum
    });
}

#[bench]
fn morton(b: &mut Bencher) {
    let mut grid: Grid = vec![1; SIZE * SIZE];
    b.iter(|| {
        let mut super_sum = 0u64;

        for y in 1..(SIZE - 1) {
            for x in 1..(SIZE - 1) {
                let index = calc_z_order(x as u16, y as u16) as usize;
                let mut sum = 0u64;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let nx = x as isize + dx;
                        let ny = y as isize + dy;
                        let index = calc_z_order(ny as u16, nx as u16) as usize;
                        sum = sum.wrapping_add(black_box(grid[index]));
                    }
                }
                grid[index] = sum;
                super_sum = super_sum.wrapping_add(sum);
            }
        }
        super_sum
    });
}

#[bench]
fn morton_orthogonal(b: &mut Bencher) {
    let mut grid: Grid = vec![1; SIZE * SIZE];
    b.iter(|| {
        let mut super_sum = 0u64;

        for x in 1..(SIZE - 1) {
            for y in 1..(SIZE - 1) {
                let index = calc_z_order(x as u16, y as u16) as usize;
                let mut sum = 0u64;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let nx = x as isize + dx;
                        let ny = y as isize + dy;
                        let index = calc_z_order(ny as u16, nx as u16) as usize;
                        sum = sum.wrapping_add(black_box(grid[index]));
                    }
                }
                grid[index] = sum;
                super_sum = super_sum.wrapping_add(sum);
            }
        }
        super_sum
    });
}

#[inline(always)]
fn calc_z_order(x: u16, y: u16) -> u32 {
    let packed = (x as u64) | ((y as u64) << 32);

    let first = (packed | (packed << 8)) & 0x00FF00FF00FF00FF;
    let second = (first | (first << 4)) & 0x0F0F0F0F0F0F0F0F;
    let third = (second | (second << 2)) & 0x3333333333333333;
    let fourth = (third | (third << 1)) & 0x5555555555555555;

    let x = fourth;
    let y = fourth >> 31;
    (x | y) as u32
}

// #[inline(always)]
// fn calc_z_order(x: u16, y: u16) -> u32 {
//     return unsafe { zorder::bmi2::index_of((x, y)) };
// }

// #[inline(always)]
// fn calc_z_order(x: u16, y: u16) -> u32 {
//     let mut x = x as u32;
//     let mut y = y as u32;
//
//     x = (x | (x << 4)) & 0x0F0F;
//     x = (x | (x << 2)) & 0x3333;
//     x = (x | (x << 1)) & 0x5555;
//
//     y = (y | (y << 4)) & 0x0F0F;
//     y = (y | (y << 2)) & 0x3333;
//     y = (y | (y << 1)) & 0x5555;
//
//     x | (y << 1)
// }
