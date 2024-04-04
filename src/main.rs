use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
    str::FromStr,
};

use anyhow::{anyhow, Context};
use clap::CommandFactory;
use colored::Colorize;
use glam::DVec3;
use map_3d::geodetic2enu;
use nmea::{
    parse_nmea_sentence,
    sentences::{parse_gga, GgaData},
    NmeaSentence,
};

#[cfg(test)]
mod tests;

#[derive(clap::Parser)]
#[command(author, version, about, long_about = None)]
struct Input {
    input_path: PathBuf,

    #[arg(short)]
    /// Return _only_ the average, with no other text. Useful for passing onto another programs or storing into a file.
    short: bool,

    #[arg(short = 'l')]
    /// Return additionally the histogram for each of the coordinates. Useful for detecting anomalies.
    show_histogram: bool,
}

fn main() -> anyhow::Result<()> {
    let input = Input::command()
        .help_template(include_str!("help_template"))
        .get_matches();

    let input_path = input.get_one::<PathBuf>("input_path").unwrap();
    let short = input.get_flag("short");
    let show_histogram = input.get_flag("show_histogram");

    let file = BufReader::new(
        File::open(input_path)
            .with_context(|| format!("Failed to read input file at {}", input_path.display()))?,
    );

    let positions = parse_file(file)?;

    let n = positions.len();
    let avg = positions.iter().copied().sum::<DVec3>() / n as f64;
    let std_dev = (positions
        .iter()
        .copied()
        .map(|r| (r - avg).powf(2.))
        .sum::<DVec3>()
        / (n - 1) as f64)
        .powf(0.5);

    let (histogram_x, division_val_x) = histogram(positions.clone(), |x| x.x, (avg, std_dev));
    let (histogram_y, division_val_y) = histogram(positions.clone(), |x| x.y, (avg, std_dev));
    let (histogram_z, division_val_z) = histogram(positions.clone(), |x| x.z, (avg, std_dev));

    let histogram_val_x = histogram_val(histogram_x);
    let histogram_val_y = histogram_val(histogram_y);
    let histogram_val_z = histogram_val(histogram_z);

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
            (avg_filtered.x + std_dev_filtered.x).to_radians(),
            (avg_filtered.y + std_dev_filtered.y).to_radians(),
            avg_filtered.z + std_dev_filtered.z,
            avg_filtered.x.to_radians(),
            avg_filtered.y.to_radians(),
            avg_filtered.z,
            map_3d::Ellipsoid::WGS84,
        );
        DVec3::from((x, y, z))
    };

    if short {
        println!("{}, {}, {}", avg_filtered.x, avg_filtered.y, avg_filtered.z);
    } else {
        println!(
            "{}",
            "Values are formatted as (latitude, longitude, altitude) unless specified otherwise.\n"
                .italic()
        );

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
        if show_histogram {
            let formatted = {
                let mut formatted = String::from_str(
                    "  Latitude (º)\t\t\t\t  Longitude (º)\t\t\t\t  Altitude(m)\n",
                )
                .unwrap();
                let iter = division_val_x
                    .iter()
                    .zip(division_val_y)
                    .zip(division_val_z)
                    .zip(histogram_val_x)
                    .zip(histogram_val_y)
                    .zip(histogram_val_z)
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
    }

    Ok(())
}

pub fn parse_file(file: BufReader<File>) -> anyhow::Result<Vec<DVec3>> {
    file.lines()
        .enumerate()
        .map(|(line_num, line)| -> anyhow::Result<Option<DVec3>> {
            let line = line.with_context(|| {
                format!("Failed to read line {} of the input file", line_num + 1)
            })?;

            if line.starts_with("$PAAG") {
                return Ok(None);
            }

            let pos = parse_line(&line)
                .map_err(|err| anyhow!(err.to_string()))
                .with_context(|| {
                    format!("Failed to parse line {} of the input file", line_num + 1)
                })?;

            Ok(pos)
        })
        .filter_map(|maybe_pos| -> Option<anyhow::Result<DVec3>> { maybe_pos.transpose() })
        .collect::<anyhow::Result<Vec<_>>>()
}

fn parse_line<'a>(line: &'a str) -> Result<Option<DVec3>, nmea::Error<'a>> {
    // https://www.sparkfun.com/datasheets/GPS/NMEA%20Reference%20Manual-Rev2.1-Dec07.pdf

    let nmea_line: NmeaSentence<'a> = parse_nmea_sentence(line)?;
    let gga_data: GgaData = match parse_gga(nmea_line) {
        Ok(gga_data) => gga_data,
        Err(nmea::Error::WrongSentenceHeader { .. }) => return Ok(None),
        Err(err) => Err(err)?,
    };

    let (Some(lat), Some(lon), Some(ele)) =
        (gga_data.latitude, gga_data.longitude, gga_data.altitude)
    else {
        return Ok(None);
    };
    Ok(Some(DVec3 {
        x: lat,
        y: lon,
        z: ele as f64,
    }))
}

fn histogram(
    mut positions: Vec<DVec3>,
    r_variable: fn(&DVec3) -> f64,
    (avg, std_dev): (DVec3, DVec3),
) -> (Vec<(i32, DVec3)>, Vec<(f64, f64)>) {
    let cutoff: i32 = 3; // measured in standard deviations
    let div: i32 = 6;
    let mut range = (-(cutoff * div)..(cutoff * div))
        .map(|i| (i as f64) / (div as f64) * r_variable(&std_dev) + r_variable(&avg))
        .enumerate()
        .peekable();
    positions.sort_by(|a, b| r_variable(a).total_cmp(&r_variable(b)));

    let division_values = range
        .clone()
        .map(|(_, x)| x)
        .zip(range.clone().map(|(_, x)| x).skip(1))
        .collect::<Vec<_>>();

    let mut histogram: Vec<(i32, DVec3)> = Vec::new();

    for pos in positions {
        while let Some((idx, val)) = range.peek() {
            if r_variable(&pos) < *val {
                histogram.push((*idx as i32, pos));
                break;
            } else {
                range.next();
            }
        }
    }

    histogram.retain(|(i, _)| *i != 0); //  Removes lower bound of data (atypical data)
    (histogram, division_values)
}

fn histogram_val(histogram: Vec<(i32, DVec3)>) -> Vec<i32> {
    let mut histogram_val = vec![0; histogram.len()];

    for idx in histogram.iter().map(|(i, _)| *i as usize) {
        histogram_val[idx] += 1;
    }
    histogram_val
}
