use crate::log::{Dataset, FieldKind, SourceConstant};

#[derive(Debug, Clone)]
pub struct Table {
    pub name: String,
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub cells: Vec<f64>,
    pub unit: String,
    pub rpm_id: usize,
    pub load_id: usize,
    /// Later table columns whose RPM breakpoint duplicated an earlier breakpoint.
    pub ignored_duplicate_rpm: usize,
}
impl Table {
    pub fn validate(&self) -> Result<(), String> {
        if self.x.len() < 2 || self.y.len() < 2 {
            return Err("Both axes need at least two breakpoints".into());
        }
        if self.x.iter().chain(&self.y).any(|v| !v.is_finite())
            || self.cells.iter().any(|v| !v.is_finite())
        {
            return Err("Table contains non-finite values".into());
        }
        if self.x.windows(2).any(|w| w[1] <= w[0]) {
            return Err(
                "RPM breakpoints are not strictly increasing (duplicate or decreasing values)"
                    .into(),
            );
        }
        if self.y.windows(2).any(|w| w[1] <= w[0]) {
            return Err(
                "Load breakpoints are not strictly increasing (duplicate or decreasing values)"
                    .into(),
            );
        }
        if self.x.len().checked_mul(self.y.len()) != Some(self.cells.len()) {
            return Err("Table dimensions do not match its axes".into());
        }
        Ok(())
    }
    pub fn position(&self, rpm: f64, load: f64) -> Option<(f64, f64)> {
        Some((
            axis_coordinate(&self.x, rpm)?,
            axis_coordinate(&self.y, load)?,
        ))
    }
}
/// Converts a physical coordinate to a fractional cell-center index, not a sample value.
pub fn axis_coordinate(axis: &[f64], value: f64) -> Option<f64> {
    if axis.len() < 2
        || !value.is_finite()
        || axis.iter().any(|x| !x.is_finite())
        || axis.windows(2).any(|w| w[1] <= w[0])
        || value < axis[0]
        || value > *axis.last()?
    {
        return None;
    }
    if value == *axis.last()? {
        return Some((axis.len() - 1) as f64);
    }
    let i = axis.partition_point(|&x| x <= value).saturating_sub(1);
    Some(i as f64 + (value - axis[i]) / (axis[i + 1] - axis[i]))
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrailPoint {
    pub x: f64,
    pub y: f64,
    pub opacity: f32,
}
/// Reconstructs dots from recorded rows only. Missing or out-of-range rows add no dot.
pub fn trail(
    table: &Table,
    data: &Dataset,
    position: f64,
    sample: Option<usize>,
    seconds: f64,
    budget: usize,
) -> Vec<TrailPoint> {
    let Some(last) = sample else {
        return Vec::new();
    };
    if !position.is_finite() || !seconds.is_finite() || seconds <= 0.0 || budget == 0 {
        return Vec::new();
    }
    let start = data.elapsed.partition_point(|&t| t < position - seconds);
    let end = (last + 1).min(data.elapsed.len());
    if start >= end {
        return Vec::new();
    }
    let step = (end - start).div_ceil(budget).max(1);
    (start..end)
        .step_by(step)
        .filter_map(|i| {
            let x = data.columns.get(table.rpm_id)?.get(i).copied().flatten()?;
            let y = data.columns.get(table.load_id)?.get(i).copied().flatten()?;
            let (x, y) = table.position(x, y)?;
            let opacity = (1.0 - (position - data.elapsed[i]) / seconds).clamp(0.0, 1.0) as f32;
            (opacity > 0.0).then_some(TrailPoint { x, y, opacity })
        })
        .collect()
}
fn constant<'a>(data: &'a Dataset, name: &str) -> Result<&'a SourceConstant, String> {
    let mut found = data
        .metadata
        .tune_constants
        .iter()
        .filter(|c| c.name == name);
    let result = found
        .next()
        .ok_or_else(|| format!("Source tune does not contain {name}"))?;
    if found.next().is_some() {
        return Err(format!("Source tune contains multiple {name} definitions"));
    }
    Ok(result)
}
fn values(c: &SourceConstant) -> Result<Vec<f64>, String> {
    c.value
        .split_whitespace()
        .map(|v| {
            v.parse::<f64>()
                .map_err(|_| format!("{} contains a nonnumeric value", c.name))
        })
        .collect()
}
fn channel(data: &Dataset, name: &str, unit: &str) -> Result<usize, String> {
    let ids: Vec<_> = data
        .field_ids(name)
        .iter()
        .copied()
        .filter(|&id| {
            data.fields[id].unit == unit
                && matches!(data.fields[id].kind, FieldKind::Numeric { .. })
        })
        .collect();
    if ids.len() != 1 {
        return Err(format!(
            "Recorded {name} ({unit}) channel is missing or ambiguous"
        ));
    }
    Ok(ids[0])
}
/// Only mappings established for the identified firmware are considered.
pub fn from_source(data: &Dataset, matrix: &str) -> Result<Table, String> {
    if !crate::analysis::derived::verified_firmware_semantics(data) {
        return Err("Table definitions are not verified for this firmware".into());
    }
    let (x_name, y_name, load_name, unit) = match matrix {
        "afrTable" => ("rpmBinsAFR", "loadBinsAFR", "FuelLoad", "AFR"),
        "veTable" => ("rpmBins", "fuelLoadBins", "FuelLoad", "%"),
        "veTable2" => ("fuelRPM2Bins", "fuelLoad2Bins", "FuelLoad2", "%"),
        _ => return Err("Unknown source table".into()),
    };
    let z = constant(data, matrix)?;
    let x = constant(data, x_name)?;
    let y = constant(data, y_name)?;
    if z.unit.as_deref() != Some(unit)
        || x.unit.as_deref() != Some("RPM")
        || y.unit.as_deref() != Some("kPa")
    {
        return Err("Source table axis or cell units are incompatible".into());
    }
    let x_values = values(x)?;
    let y_values = values(y)?;
    let cells = values(z)?;
    if z.cols != Some(x_values.len())
        || z.rows != Some(y_values.len())
        || x.rows != Some(x_values.len())
        || x.cols != Some(1)
        || y.rows != Some(y_values.len())
        || y.cols != Some(1)
    {
        return Err("Source table dimensions do not match axis metadata".into());
    }
    // Validate the table before looking for the operating-point channels, so an invalid
    // source grid is reported even when the secondary load channel is also absent.
    if x_values.iter().any(|v| !v.is_finite()) || cells.iter().any(|v| !v.is_finite()) {
        return Err("Table contains non-finite values".into());
    }
    if x_values.windows(2).any(|w| w[1] < w[0]) {
        return Err("RPM breakpoints are decreasing".into());
    }
    let kept: Vec<usize> = (0..x_values.len())
        .filter(|&i| i == 0 || x_values[i] != x_values[i - 1])
        .collect();
    let ignored_duplicate_rpm = x_values.len() - kept.len();
    let mut reduced_cells = Vec::with_capacity(y_values.len() * kept.len());
    for row in 0..y_values.len() {
        for &col in &kept {
            reduced_cells.push(cells[row * x_values.len() + col]);
        }
    }
    let reduced_x = kept.iter().map(|&i| x_values[i]).collect();
    let mut table = Table {
        name: matrix.into(),
        x: reduced_x,
        y: y_values,
        cells: reduced_cells,
        unit: unit.into(),
        rpm_id: 0,
        load_id: 0,
        ignored_duplicate_rpm,
    };
    table.validate()?;
    if matrix == "veTable2" {
        return Err("Secondary fuel-load channel identity is not verified in recorded logs".into());
    }
    table.rpm_id = channel(data, "RPM", "rpm")?;
    table.load_id = channel(data, load_name, "kPa")?;
    Ok(table)
}
