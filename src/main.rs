use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
    str::FromStr,
};

use anyhow::Context;
use clap::CommandFactory;
use colored::Colorize;
use glam::DVec3;
use map_3d::geodetic2enu;

#[derive(clap::Parser)]
#[command(author, version, about, long_about = None)]
struct Input {
    input_path: PathBuf,

    #[arg(short)]
    /// Return _only_ the average, with no other text. Useful for passing onto another programs or storing into a file.
    short: bool,
}

fn main() -> anyhow::Result<()> {
    let input = Input::command()
        .help_template(include_str!("help_template"))
        .get_matches();

    let input_path = input.get_one::<PathBuf>("input_path").unwrap();
    let short = input.get_flag("short");

    let file = BufReader::new(
        File::open(&input_path)
            .with_context(|| format!("Failed to read input file at {}", input_path.display()))?,
    );

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

    let (histogram_x, division_val_x) = histogram(&positions, |x| x.x);
    let (histogram_y, division_val_y) = histogram(&positions, |x| x.y);
    let (histogram_z, division_val_z) = histogram(&positions, |x| x.z);

    let histogram_val_x: Vec<_> = histogram_x.iter().map(|x| x.len()).collect();
    let histogram_val_y: Vec<_> = histogram_y.iter().map(|y| y.len()).collect();
    let histogram_val_z: Vec<_> = histogram_z.iter().map(|z| z.len()).collect();

    let positions_filtered = positions
        .iter()
        .filter(|x| {
            let cutoff: f64 = 3.;

            let n = positions.len();
            let avg = positions.iter().copied().sum::<DVec3>() / n as f64;
            let std_dev = (positions
                .iter()
                .copied()
                .map(|r| (r - avg).powf(2.))
                .sum::<DVec3>()
                / (n - 1) as f64)
                .powf(0.5);

            x.x > avg.x - cutoff * std_dev.x
                && x.x < avg.x + cutoff * std_dev.x
                && x.y > avg.y - cutoff * std_dev.y
                && x.y < avg.y + cutoff * std_dev.y
                && x.z > avg.z - cutoff * std_dev.z
                && x.z < avg.z + cutoff * std_dev.z
        })
        .copied()
        .collect::<Vec<DVec3>>();

    let m = positions.len();
    let n = positions_filtered.len();
    let avg = positions_filtered.iter().copied().sum::<DVec3>() / n as f64;
    let std_dev = (positions_filtered
        .iter()
        .copied()
        .map(|r| (r - avg).powf(2.))
        .sum::<DVec3>()
        / (n - 1) as f64)
        .powf(0.5);
    let std_dev_m = {
        let (y, x, z) = geodetic2enu(
            (avg + std_dev).x,
            (avg + std_dev).y,
            (avg + std_dev).z,
            avg.x,
            avg.y,
            avg.z,
            map_3d::Ellipsoid::WGS84,
        );
        DVec3::from((x, y, z))
    };

    if short {
        println!("{}, {}, {}", avg.x, avg.y, avg.z);
    } else {
        let formatted = format!("({:.4}º, {:.4}º, {:.1}m)", avg.x, avg.y, avg.z).bold();
        let formatted_raw = format!("({}, {}, {})", avg.x, avg.y, avg.z).italic();
        println!("Average: {formatted} {formatted_raw}\n");

        let formatted = format!("({} discarded)", m - n).italic();
        println!("Number of entries: {m} {}", formatted);
        let formatted = format!("({:.6}º, {:.6}º, {:.3}m)", std_dev.x, std_dev.y, std_dev.z);
        let formatted_m =
            format!("Horizontally: ~({:.2}m, {:.2}m)", std_dev_m.x, std_dev_m.y).italic();
        println!("Standard deviation: {formatted} {formatted_m}");

        let formatted = {
            let mut formatted =
                String::from_str("  Latitude (º)            Longitude (º)           Altitude(m)\n")
                    .unwrap();
            let mut val_x = division_val_x.iter(); // All these iterators have the same length (2 * cutoff * div ) from histogram
            let mut val_y = division_val_y.iter();
            let mut val_z = division_val_z.iter();
            let mut y = histogram_val_y.iter();
            let mut z = histogram_val_z.iter();
            for x in histogram_val_x.iter() {
                formatted.push_str(
                    format!(
                        "{}\t{:.6}\t{}\t{:.6}\t{}\t{:.6}\n",
                        x,
                        val_x.next().unwrap(),
                        y.next().unwrap(),
                        val_y.next().unwrap(),
                        z.next().unwrap(),
                        val_z.next().unwrap()
                    )
                    .as_str(),
                );
            }
            formatted
        };
        println!("Histogram values:\n {} ", formatted);
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
                    "Failed to parse longitude; \
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

fn histogram(positions: &Vec<DVec3>, r_variable: fn(&DVec3) -> f64) -> (Vec<Vec<DVec3>>, Vec<f64>) {
    let mut position_set: Vec<DVec3> = positions.clone();

    let n = position_set.len();
    let avg = position_set.iter().copied().sum::<DVec3>() / n as f64;
    let std_dev = (position_set
        .iter()
        .copied()
        .map(|r| (r - avg).powf(2.))
        .sum::<DVec3>()
        / (n - 1) as f64)
        .powf(0.5);

    let cutoff: i32 = 3; // measured in standard deviations
    let div: i32 = 6;
    let mut range = (-(cutoff * div)..(cutoff * div))
        .into_iter()
        .map(|i| (i as f64) / (div as f64) * r_variable(&std_dev) + r_variable(&avg))
        .peekable();
    position_set.sort_by(|a, b| {
        r_variable(a)
            .partial_cmp(&r_variable(b))
            .expect("Histogram: Incomparable values")
    });

    let division_values = range.clone().collect::<Vec<f64>>();
    let mut histogram: Vec<Vec<DVec3>> = Vec::new();
    let mut division: Vec<DVec3> = Vec::new();

    for pos in position_set {
        while range.peek().is_some() {
            if r_variable(&pos) < *range.peek().unwrap() {
                division.push(pos);
                break;
            } else {
                histogram.push(division.clone());
                division.clear();
                range.next();
            }
        }
    }

    while range.next().is_some() {
        histogram.push(division.clone());
        division.clear();
    }
    histogram.remove(0); //  Removes lower bound of data (atypical data)
    (histogram, division_values)
}
