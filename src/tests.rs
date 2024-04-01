use std::{fs::File, io::BufReader};

use anyhow::Context;

use crate::parse_file;

#[test]
fn read_correct_file() {
    let input_path = "tests/assets/1";
    let file = BufReader::new(
        File::open(&input_path)
            .with_context(|| format!("Failed to read input file at {}", input_path))
            .unwrap(),
    );

    let positions = parse_file(file).unwrap();
}

#[test]
fn read_blank_file() {
    let input_path = "tests/assets/2";
    let file = BufReader::new(
        File::open(&input_path)
            .with_context(|| format!("Failed to read input file at {}", input_path))
            .unwrap(),
    );

    let positions = parse_file(file).unwrap();
}

#[test]
#[should_panic]
fn read_broken_file() {
    let input_path = "tests/assets/1_broken";
    let file = BufReader::new(
        File::open(&input_path)
            .with_context(|| format!("Failed to read input file at {}", input_path))
            .unwrap(),
    );

    let positions = parse_file(file).unwrap();
}
