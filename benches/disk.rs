#![feature(test)]
#[cfg(test)]
extern crate test;

use flate2::{Compression, write::GzEncoder, read::GzDecoder};
use test::Bencher;

use std::{fs::File, io::{Write, Read}};

fn gen_data() -> Vec<u8> {
    let mut data = vec![0u8; 1 * 1_024 * 1_024];
    for (index, datum) in data.iter_mut().enumerate() {
        *datum = (index * 17 + index % 13) as u8;
    }
    data
}

#[bench]
fn write_1mb_block(b: &mut Bencher) {
    let data = gen_data();
    b.iter(|| {
        let file = File::create("./temp_file").unwrap();
        let mut writer = GzEncoder::new(file, Compression::default());
        writer.write_all(&data).unwrap();
    });
    std::fs::remove_file("./temp_file").unwrap();
}

#[bench]
fn read_1mb_block(b: &mut Bencher) {
    let mut data = gen_data();
    let file = File::create("./temp_file").unwrap();
    let mut writer = GzEncoder::new(file, Compression::default());
    writer.write_all(&data).unwrap();
    drop(writer);
    b.iter(|| {
        data.clear();
        let file = File::open("./temp_file").unwrap();
        let mut reader = GzDecoder::new(file);
        reader.read_to_end(&mut data).unwrap();
    });
    std::fs::remove_file("./temp_file").unwrap();
}
