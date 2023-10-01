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

    let n = positions.len();
    let avg = positions.iter().copied().sum::<DVec3>() / n as f64;
    let std_dev = (positions
        .iter()
        .copied()
        .map(|r| (r - avg).powf(2.))
        .sum::<DVec3>()
        / (n - 1) as f64)
        .powf(0.5);

    let (histogram_x, division_val_x) = histogram(&positions, |x| x.x, (avg, std_dev));
    let (histogram_y, division_val_y) = histogram(&positions, |x| x.y, (avg, std_dev));
    let (histogram_z, division_val_z) = histogram(&positions, |x| x.z, (avg, std_dev));

    let histogram_val_x = {
        let mut histogram_val = vec![0; division_val_x.len()];

        for idx in histogram_x.iter().map(|(i, _)| *i as usize) {
            histogram_val[idx] = histogram_val[idx] + 1;
        }
        histogram_val
    };
    let histogram_val_y = {
        let mut histogram_val = vec![0; division_val_y.len()];

        for idx in histogram_y.iter().map(|(i, _)| *i as usize) {
            histogram_val[idx] = histogram_val[idx] + 1;
        }
        histogram_val
    };
    let histogram_val_z = {
        let mut histogram_val = vec![0; division_val_z.len()];

        for idx in histogram_z.iter().map(|(i, _)| *i as usize) {
            histogram_val[idx] = histogram_val[idx] + 1;
        }
        histogram_val
    };

    let positions_filtered = positions
        .iter()
        .filter(|x| {
            let cutoff: f64 = 3.;

            x.x > avg.x - cutoff * std_dev.x
                && x.x < avg.x + cutoff * std_dev.x
                && x.y > avg.y - cutoff * std_dev.y
                && x.y < avg.y + cutoff * std_dev.y
                && x.z > avg.z - cutoff * std_dev.z
                && x.z < avg.z + cutoff * std_dev.z
        })
        .copied()
        .collect::<Vec<DVec3>>();

    let n = positions.len();
    let n_filtered = positions_filtered.len();
    let avg_filtered = positions_filtered.iter().copied().sum::<DVec3>() / n_filtered as f64;
    let std_dev_filtered = (positions_filtered
        .iter()
        .copied()
        .map(|r| (r - avg_filtered).powf(2.))
        .sum::<DVec3>()
        / (n_filtered - 1) as f64)
        .powf(0.5);
    let std_dev_m = {
        let (y, x, z) = geodetic2enu(
            (avg_filtered + std_dev_filtered).x,
            (avg_filtered + std_dev_filtered).y,
            (avg_filtered + std_dev_filtered).z,
            avg_filtered.x,
            avg_filtered.y,
            avg_filtered.z,
            map_3d::Ellipsoid::WGS84,
        );
        DVec3::from((x, y, z))
    };

    if short {
        println!("{}, {}, {}", avg_filtered.x, avg_filtered.y, avg_filtered.z);
    } else {
        let formatted = format!(
            "({:.4}º, {:.4}º, {:.1}m)",
            avg_filtered.x, avg_filtered.y, avg_filtered.z
        )
        .bold();
        let formatted_raw = format!(
            "({}, {}, {})",
            avg_filtered.x, avg_filtered.y, avg_filtered.z
        )
        .italic();
        println!("Average: {formatted} {formatted_raw}\n");

        let formatted = format!("({} after filter)", n_filtered).italic();
        println!("Number of entries: {n} {}", formatted);
        let formatted = format!(
            "({:.6}º, {:.6}º, {:.3}m)",
            std_dev_filtered.x, std_dev_filtered.y, std_dev_filtered.z
        );
        let formatted_m =
            format!("Horizontally: ~({:.2}m, {:.2}m)", std_dev_m.x, std_dev_m.y).italic();
        println!("Standard deviation: {formatted} {formatted_m}");

        let formatted = {
            let mut formatted =
                String::from_str("  Latitude (º)\t\t\t\t  Longitude (º)\t\t\t\t  Altitude(m)\n")
                    .unwrap();
            let iter = division_val_x
                .iter()
                .zip(division_val_y.iter())
                .zip(division_val_z.iter())
                .zip(histogram_val_x.iter())
                .zip(histogram_val_y.iter())
                .zip(histogram_val_z.iter())
                .map(
                    |(
                        (((((inf_x, sup_x), (inf_y, sup_y)), (inf_z, sup_z)), hist_x), hist_y),
                        hist_z,
                    )| {
                        (
                            inf_x, sup_x, inf_y, sup_y, inf_z, sup_z, hist_x, hist_y, hist_z,
                        )
                    },
                );
            for (inf_x, sup_x, inf_y, sup_y, inf_z, sup_z, hist_x, hist_y, hist_z) in iter {
                formatted.push_str(
                    format!(
                        "{hist_x}\t({:.6} , {:.6})\t\t{hist_y}\t({:.6} , {:.6})\t\t{hist_z}\t({:.6} , {:.6})\n",
                        inf_x, sup_x, inf_y, sup_y, inf_z, sup_z
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

fn histogram(
    positions: &Vec<DVec3>,
    r_variable: fn(&DVec3) -> f64,
    (avg, std_dev): (DVec3, DVec3),
) -> (Vec<(i32, DVec3)>, Vec<(f64, f64)>) {
    let mut position_set: Vec<DVec3> = positions.clone();

    let cutoff: i32 = 3; // measured in standard deviations
    let div: i32 = 6;
    let mut range = (-(cutoff * div)..(cutoff * div))
        .into_iter()
        .map(|i| (i as f64) / (div as f64) * r_variable(&std_dev) + r_variable(&avg))
        .enumerate()
        .peekable();
    position_set.sort_by(|a, b| {
        r_variable(a)
            .partial_cmp(&r_variable(b))
            .expect("Histogram: Incomparable values")
    });

    let division_values = range
        .clone()
        .map(|(_, x)| x)
        .zip(range.clone().map(|(_, x)| x).skip(1))
        .collect::<Vec<_>>();

    let mut histogram: Vec<(i32, DVec3)> = Vec::new();

    for pos in position_set {
        while let Some((idx, val)) = range.peek() {
            if r_variable(&pos) < *val {
                histogram.push((*idx as i32, pos));
                break;
            } else {
                range.next();
            }
        }
    }

    let histogram = histogram
        .into_iter()
        .filter(|(i, _)| *i != 0) //  Removes lower bound of data (atypical data)
        .collect::<Vec<(i32, DVec3)>>();
    (histogram, division_values)
}
