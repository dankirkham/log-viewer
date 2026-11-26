use std::{env, fs, process::Command, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .ok_or("usage: cargo run --release --example measure -- LOG.mlg")?;
    let read_start = Instant::now();
    let bytes = fs::read(&path)?;
    let read_ms = read_start.elapsed().as_secs_f64() * 1000.0;
    let mut timings = Vec::with_capacity(9);
    let mut summary = None;
    for _ in 0..9 {
        let started = Instant::now();
        let dataset = log_viewer::log::parse(&bytes).map_err(std::io::Error::other)?;
        timings.push(started.elapsed().as_secs_f64() * 1000.0);
        let rpm = dataset.field_ids("RPM").first().map(|&id| {
            (
                dataset.columns[id].first().copied().flatten(),
                dataset.columns[id].last().copied().flatten(),
            )
        });
        summary = Some((dataset.timestamps.len(), dataset.fields.len(), rpm));
    }
    timings.sort_by(f64::total_cmp);
    let (rows, fields, rpm) = summary.unwrap();
    let cpu = fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name\t"))
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "CPU model unavailable".to_owned());
    let kernel = Command::new("uname")
        .arg("-sr")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|| "kernel unavailable".into());
    println!(
        "file={path} bytes={} rows={rows} fields={fields}",
        bytes.len()
    );
    if let Some((first, last)) = rpm {
        println!("rpm_first={first:?} rpm_last={last:?}");
    }
    println!("machine={cpu}; {kernel}");
    println!("read_ms={read_ms:.3}; parse_ms_median={:.3}; parse_ms_min={:.3}; parse_ms_max={:.3}; iterations=9",
        timings[4], timings[0], timings[8]);
    Ok(())
}
