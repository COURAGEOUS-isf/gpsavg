use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
};

use anyhow::Context;
use clap::Parser;
use colored::Colorize;
use glam::DVec3;

/// The Earth's radius, in meters.
const EARTH_RADIUS: f64 = 6371000.;

#[derive(clap::Parser)]
#[command(author, version, about, long_about = None)]
struct Input {
    input_path: PathBuf,

    #[arg(short)]
    /// Return _only_ the average, with no other text. Useful for passing onto another programs or storing into a file.
    short: bool,
}

fn main() -> anyhow::Result<()> {
    let input = Input::parse();

    let file = BufReader::new(File::open(&input.input_path).with_context(|| {
        format!(
            "Failed to read input file at {}",
            input.input_path.display()
        )
    })?);

    let positions = file
        .lines()
        .enumerate()
        .map(|(line_num, line)| -> anyhow::Result<Option<DVec3>> {
            let line = line.with_context(|| {
                format!("Failed to read line {} of the input file", line_num + 1)
            })?;

            let pos = parse_line(&line).with_context(|| {
                format!("Failed to parse line {} of the input file", line_num + 1)
            })?;

            Ok(pos)
        })
        .filter_map(|maybe_pos| -> Option<anyhow::Result<DVec3>> { maybe_pos.transpose() })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let n = positions.len();
    let avg = positions.iter().copied().sum::<DVec3>() / n as f64;
    let std_dev = (positions
        .iter()
        .copied()
        .map(|r| (r - avg).powf(2.))
        .sum::<DVec3>()
        / (n - 1) as f64)
        .powf(0.5);

    if input.short {
        println!("{}, {}, {}", avg.x, avg.y, avg.z);
    } else {
        let formatted = format!("({:.4}º, {:.4}º, {:.1}m)", avg.x, avg.y, avg.z).bold();
        let formatted_raw = format!("({}, {}, {})", avg.x, avg.y, avg.z).italic();
        println!("Average: {formatted} {formatted_raw}\n");

        println!("Number of entries: {n}");
        let formatted = format!("({:.6}º, {:.6}º, {:.3}m)", std_dev.x, std_dev.y, std_dev.z);
        let formatted_m = format!(
            "Horizontally: ~({:.2}m, {:.2}m)",
            std_dev.x * EARTH_RADIUS,
            std_dev.y * EARTH_RADIUS
        )
        .italic();
        println!("Standard deviation: {formatted} {formatted_m}");
    }

    Ok(())
}

fn parse_line(line: &str) -> anyhow::Result<Option<DVec3>> {
    // https://www.sparkfun.com/datasheets/GPS/NMEA%20Reference%20Manual-Rev2.1-Dec07.pdf

    let mut params = line.split(',');
    match params.next() {
        Some("$GPGGA") => {
            let params = params.collect::<Vec<_>>();
            let params: [&str; 14] = params.try_into().map_err(|params: Vec<_>| {
                anyhow::anyhow!(
                    "Invalid GPGGA message length; Expecting 14 but got {} fields",
                    params.len()
                )
            })?;

            let lat = (|| -> anyhow::Result<f64> {
                let lat = params[1].split_at(2);
                let lat_deg: f64 = lat.0.parse::<f64>()?;
                let lat_min: f64 = lat.1.parse::<f64>()?;
                Ok(lat_deg + lat_min / 60.)
            })()
            .with_context(|| {
                format!(
                    "Failed to parse latitude; \
                Expecting a number formatted as ddmm.mmmm but got '{}'",
                    params[1]
                )
            })?;
            let ns = params[2];
            let lon = (|| -> anyhow::Result<f64> {
                let lon = params[3].split_at(3);
                let lon_deg: f64 = lon.0.parse::<f64>()?;
                let lon_min: f64 = lon.1.parse::<f64>()?;
                Ok(lon_deg + lon_min / 60.)
            })()
            .with_context(|| {
                format!(
                    "Failed to parse latitude; \
            Expecting a number formatted as dddmm.mmmm but got '{}'",
                    params[3]
                )
            })?;
            let ew = params[4];
            let ele: f64 = params[8].parse()?;

            let lat = match ns {
                "N" => lat,
                "S" => -lat,
                ns => {
                    return Err(anyhow::anyhow!(
                        "Invalid N/S indicator; Expecting 'N' or 'S', found '{}'",
                        ns
                    ));
                }
            };
            let lon = match ew {
                "E" => lon,
                "W" => -lon,
                ew => {
                    return Err(anyhow::anyhow!(
                        "Invalid E/W indicator; Expecting 'E' or 'W', found '{}'",
                        ew
                    ));
                }
            };

            Ok(Some(DVec3 {
                x: lat,
                y: lon,
                z: ele,
            }))
        }
        _ => Ok(None),
    }
}
