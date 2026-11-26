use super::schema::Tune;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq)]
pub enum FieldKind {
    Numeric {
        scale: f64,
        transform: f64,
        digits: i8,
    },
    Bits {
        names: Vec<String>,
        style: u8,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Provenance {
    Source,
    Derived {
        formula: &'static str,
        dependencies: Vec<usize>,
    },
}

#[derive(Debug, Clone)]
pub struct Field {
    pub id: usize,
    pub name: String,
    pub unit: String,
    pub category: String,
    pub source_type: Option<u8>,
    pub provenance: Provenance,
    pub display_style: u8,
    pub kind: FieldKind,
}

#[derive(Debug, Clone)]
pub struct SourceConstant {
    pub name: String,
    pub unit: Option<String>,
    pub rows: Option<usize>,
    pub cols: Option<usize>,
    pub value: String,
}

#[derive(Debug, Default, Clone)]
pub struct Metadata {
    pub unix_start: Option<u32>,
    pub info: Vec<String>,
    pub tune_constants: Vec<SourceConstant>,
    pub tune_diagnostic: Option<String>,
    pub marker_count: usize,
    pub derived_diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Statistics {
    pub min: f64,
    pub max: f64,
    pub median: f64,
}

#[derive(Debug)]
pub struct Dataset {
    pub fields: Vec<Field>,
    pub columns: Vec<Vec<Option<f64>>>,
    pub timestamps: Vec<f64>,
    pub elapsed: Vec<f64>,
    pub metadata: Metadata,
    by_name: HashMap<String, Vec<usize>>,
    stats: Vec<OnceLock<Option<Statistics>>>,
}
impl Dataset {
    pub fn field_ids(&self, name: &str) -> &[usize] {
        self.by_name.get(name).map(Vec::as_slice).unwrap_or(&[])
    }
    pub fn sample_at(&self, time: f64) -> Option<usize> {
        if self.elapsed.is_empty() || !time.is_finite() {
            return None;
        }
        Some(
            self.elapsed
                .partition_point(|&t| t <= time)
                .saturating_sub(1),
        )
    }
    pub fn time_range(&self, start: f64, end: f64) -> Range<usize> {
        if !start.is_finite() || !end.is_finite() || start > end {
            return 0..0;
        }
        self.elapsed.partition_point(|&t| t < start)..self.elapsed.partition_point(|&t| t <= end)
    }
    pub fn statistics(&self, id: usize) -> Option<Statistics> {
        let field = self.fields.get(id)?;
        if !matches!(field.kind, FieldKind::Numeric { .. }) {
            return None;
        }
        *self.stats[id].get_or_init(|| {
            let mut values: Vec<f64> = self.columns[id].iter().flatten().copied().collect();
            if values.is_empty() {
                return None;
            }
            values.sort_unstable_by(f64::total_cmp);
            let n = values.len();
            let median = if n % 2 == 0 {
                values[n / 2 - 1] / 2.0 + values[n / 2] / 2.0
            } else {
                values[n / 2]
            };
            Some(Statistics {
                min: values[0],
                max: values[n - 1],
                median,
            })
        })
    }
    pub(crate) fn push_derived(
        &mut self,
        name: &str,
        unit: &str,
        formula: &'static str,
        dependencies: Vec<usize>,
        values: Vec<Option<f64>>,
    ) {
        let id = self.fields.len();
        self.fields.push(Field {
            id,
            name: name.into(),
            unit: unit.into(),
            category: "Derived".into(),
            source_type: None,
            provenance: Provenance::Derived {
                formula,
                dependencies,
            },
            display_style: 0,
            kind: FieldKind::Numeric {
                scale: 1.0,
                transform: 0.0,
                digits: 2,
            },
        });
        self.columns.push(values);
        self.by_name.entry(name.into()).or_default().push(id);
        self.stats.push(OnceLock::new());
    }
    pub fn duration(&self) -> f64 {
        self.elapsed.last().copied().unwrap_or(0.0)
    }
}

fn u16be(b: &[u8]) -> u16 {
    u16::from_be_bytes([b[0], b[1]])
}
fn u32be(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}
fn f32be(b: &[u8]) -> f64 {
    f32::from_bits(u32be(b)) as f64
}
fn string(b: &[u8]) -> Result<String, String> {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    std::str::from_utf8(&b[..end])
        .map(str::to_owned)
        .map_err(|_| "Non-UTF-8 field metadata is unsupported".into())
}
fn size(t: u8) -> Option<usize> {
    match t {
        0 | 1 | 16 => Some(1),
        2 | 3 | 17 => Some(2),
        4 | 5 | 7 | 18 => Some(4),
        6 => Some(8),
        _ => None,
    }
}
fn raw(t: u8, b: &[u8]) -> f64 {
    match t {
        0 | 16 => b[0] as f64,
        1 => (b[0] as i8) as f64,
        2 | 17 => u16be(b) as f64,
        3 => (u16be(b) as i16) as f64,
        4 | 18 => u32be(b) as f64,
        5 => (u32be(b) as i32) as f64,
        6 => i64::from_be_bytes(b.try_into().expect("validated width")) as f64,
        7 => f32be(b),
        _ => unreachable!("validated type"),
    }
}
fn tune(bytes: &[u8], metadata: &mut Metadata) {
    let marker = b"NEW_INFO_PROVIDER,LogStart_MAIN_TUNE,";
    let Some(m) = bytes.windows(marker.len()).position(|w| w == marker) else {
        return;
    };
    let rest = &bytes[m..];
    let Some(start) = rest.windows(5).position(|w| w == b"<?xml") else {
        metadata.tune_diagnostic = Some("Embedded tune marker has no XML document".into());
        return;
    };
    let Some(end) = rest[start..].windows(6).position(|w| w == b"</msq>") else {
        metadata.tune_diagnostic = Some("Embedded tune XML is incomplete".into());
        return;
    };
    let xml = &rest[start..start + end + 6];
    if xml.windows(9).any(|w| w.eq_ignore_ascii_case(b"<!DOCTYPE"))
        || xml.windows(8).any(|w| w.eq_ignore_ascii_case(b"<!ENTITY"))
    {
        metadata.tune_diagnostic =
            Some("Embedded tune contains a forbidden DTD or entity declaration".into());
        return;
    }
    let declaration_end = xml.windows(2).position(|w| w == b"?>").unwrap_or(0);
    let declaration = String::from_utf8_lossy(&xml[..declaration_end]).to_ascii_lowercase();
    let xml = if declaration.contains("encoding") && declaration.contains("iso-8859-1") {
        xml.iter().map(|&b| char::from(b)).collect::<String>()
    } else if !declaration.contains("encoding") || declaration.contains("utf-8") {
        match std::str::from_utf8(xml) {
            Ok(text) => text.to_owned(),
            Err(_) => {
                metadata.tune_diagnostic = Some("Embedded tune is not valid UTF-8".into());
                return;
            }
        }
    } else {
        metadata.tune_diagnostic = Some("Unsupported embedded tune XML encoding".into());
        return;
    };
    match quick_xml::de::from_str::<Tune>(&xml) {
        Ok(t) => {
            metadata.tune_constants = t
                .pages
                .into_iter()
                .flat_map(|p| p.constants)
                .map(|c| SourceConstant {
                    name: c.name,
                    unit: c.units,
                    rows: c.rows,
                    cols: c.cols,
                    value: c.value,
                })
                .collect()
        }
        Err(e) => metadata.tune_diagnostic = Some(format!("Cannot decode embedded tune XML: {e}")),
    }
}

/// Parse the supported binary log container. Unknown revisions and block types are rejected.
pub fn parse(bytes: &[u8]) -> Result<Dataset, String> {
    const SIGNATURE: [u8; 6] = [0x4d, 0x4c, 0x56, 0x4c, 0x47, 0x00];
    if bytes.len() < 24 || bytes[..6] != SIGNATURE {
        return Err("Not a supported binary log (unrecognized header signature)".into());
    }
    let version = u16be(&bytes[6..8]);
    if version != 2 {
        return Err(format!("Unsupported binary log revision {version}"));
    }
    let info_start = u32be(&bytes[12..16]) as usize;
    let data_start = u32be(&bytes[16..20]) as usize;
    let record_len = u16be(&bytes[20..22]) as usize;
    let count = u16be(&bytes[22..24]) as usize;
    let desc_end = 24usize
        .checked_add(count * 89)
        .ok_or("Field count overflow")?;
    if desc_end > bytes.len()
        || data_start < desc_end
        || data_start > bytes.len()
        || (info_start != 0 && (info_start < desc_end || info_start > data_start))
    {
        return Err("Invalid header offsets or truncated field descriptors".into());
    }
    let mut fields = Vec::with_capacity(count);
    let mut widths = Vec::with_capacity(count);
    let mut total = 0usize;
    let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
    for id in 0..count {
        let d = &bytes[24 + id * 89..24 + (id + 1) * 89];
        let t = d[0];
        let width = size(t).ok_or_else(|| format!("Unsupported field type {t} for field {id}"))?;
        total += width;
        widths.push(width);
        let name = string(&d[1..35])?;
        let unit = string(&d[35..45])?;
        let kind = if t >= 16 {
            let mut pos = u32be(&d[47..51]) as usize;
            let bits = d[51] as usize;
            if bits > width * 8 || (bits > 0 && (pos < desc_end || pos >= data_start)) {
                return Err(format!("Invalid bit names for field {id}"));
            }
            let mut names = Vec::with_capacity(bits);
            for _ in 0..bits {
                let remaining = bytes
                    .get(pos..data_start)
                    .ok_or("Invalid bit name offset")?;
                let len = remaining
                    .iter()
                    .position(|&b| b == 0)
                    .ok_or("Unterminated bit name")?;
                names.push(string(&remaining[..len])?);
                pos += len + 1;
            }
            FieldKind::Bits {
                names,
                style: d[46],
            }
        } else {
            FieldKind::Numeric {
                scale: f32be(&d[46..50]),
                transform: f32be(&d[50..54]),
                digits: d[54] as i8,
            }
        };
        by_name.entry(name.clone()).or_default().push(id);
        fields.push(Field {
            id,
            name,
            unit,
            category: string(&d[55..89])?,
            source_type: Some(t),
            provenance: Provenance::Source,
            display_style: d[45],
            kind,
        });
    }
    if total != record_len {
        return Err(format!(
            "Record length {record_len} does not match field widths {total}"
        ));
    }
    let times: Vec<usize> = fields
        .iter()
        .filter(|f| {
            f.name == "Time" && f.unit == "s" && matches!(f.kind, FieldKind::Numeric { .. })
        })
        .map(|f| f.id)
        .collect();
    if times.len() != 1 {
        return Err(
            "Expected exactly one numeric Time field with units 's'; timestamp units are ambiguous"
                .into(),
        );
    }
    let mut metadata = Metadata {
        unix_start: match u32be(&bytes[8..12]) {
            0 => None,
            n => Some(n),
        },
        ..Metadata::default()
    };
    if info_start != 0 {
        let info = &bytes[info_start..data_start];
        metadata.info = info
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .take(2)
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect();
        tune(info, &mut metadata);
    }
    let mut columns = vec![Vec::new(); count];
    let mut pos = data_start;
    let mut row = 0usize;
    while pos < bytes.len() {
        match bytes[pos] {
            0 => {
                let end = pos
                    .checked_add(record_len + 5)
                    .ok_or("Record length overflow")?;
                if end > bytes.len() {
                    return Err(format!("Truncated sample block at byte {pos}"));
                }
                let payload = &bytes[pos + 4..pos + 4 + record_len];
                if payload.iter().fold(0u8, |sum, &b| sum.wrapping_add(b)) != bytes[end - 1] {
                    return Err(format!("Checksum mismatch in sample {row} at byte {pos}"));
                }
                let mut off = 0;
                for (i, f) in fields.iter().enumerate() {
                    if f.source_type == Some(6) {
                        let integer = i64::from_be_bytes(
                            payload[off..off + 8].try_into().expect("validated width"),
                        );
                        if integer.unsigned_abs() > (1u64 << 53) {
                            return Err(format!("Sample {row}, field {} contains a 64-bit integer that cannot be represented exactly", f.name));
                        }
                    }
                    let v = raw(
                        f.source_type.expect("source field"),
                        &payload[off..off + widths[i]],
                    );
                    off += widths[i];
                    let v = match f.kind {
                        FieldKind::Numeric {
                            scale, transform, ..
                        } => (v + transform) * scale,
                        FieldKind::Bits { .. } => v,
                    };
                    columns[i].push(v.is_finite().then_some(v));
                }
                row += 1;
                pos = end;
            }
            1 => {
                if bytes.len() - pos < 54 {
                    return Err(format!("Truncated marker at byte {pos}"));
                }
                metadata.marker_count += 1;
                pos += 54;
            }
            t => return Err(format!("Unsupported block type {t} at byte {pos}")),
        }
    }
    let timestamps: Vec<f64> = columns[times[0]]
        .iter()
        .enumerate()
        .map(|(i, v)| v.ok_or_else(|| format!("Missing or non-finite timestamp at sample {i}")))
        .collect::<Result<_, _>>()?;
    for (i, pair) in timestamps.windows(2).enumerate() {
        if pair[1] < pair[0] {
            return Err(format!(
                "Timestamp decreases at sample {} ({} to {})",
                i + 1,
                pair[0],
                pair[1]
            ));
        }
    }
    let first = timestamps.first().copied().unwrap_or(0.0);
    let elapsed = timestamps.iter().map(|t| t - first).collect();
    let mut data = Dataset {
        fields,
        columns,
        timestamps,
        elapsed,
        metadata,
        by_name,
        stats: (0..count).map(|_| OnceLock::new()).collect(),
    };
    crate::analysis::derived::install(&mut data);
    Ok(data)
}

/// Copy recorded blocks in an elapsed-time interval into a new valid container.
/// The interval is inclusive and uses the same zero-based time axis as the timeline.
pub fn crop(bytes: &[u8], dataset: &Dataset, start: f64, end: f64) -> Result<Vec<u8>, String> {
    if !start.is_finite() || !end.is_finite() || start < 0.0 || end < start {
        return Err("Crop range must be finite, nonnegative, and ordered".into());
    }
    if dataset.timestamps.is_empty() {
        return Err("Cannot crop a log with no recorded samples".into());
    }
    if start > dataset.duration() {
        return Err("Crop range does not contain any recorded samples".into());
    }
    let first = start.min(dataset.duration());
    let last = end.min(dataset.duration());
    let selected = dataset.time_range(first, last);
    if selected.is_empty() {
        return Err("Crop range does not contain any recorded samples".into());
    }

    if bytes.len() < 24 || bytes[..6] != [0x4d, 0x4c, 0x56, 0x4c, 0x47, 0x00] {
        return Err("Source bytes are not a supported binary log".into());
    }
    let data_start = u32be(&bytes[16..20]) as usize;
    let record_len = u16be(&bytes[20..22]) as usize;
    if data_start > bytes.len() {
        return Err("Source log has an invalid data offset".into());
    }
    let mut output = bytes[..data_start].to_vec();
    let mut pos = data_start;
    let mut row = 0usize;
    while pos < bytes.len() {
        match bytes[pos] {
            0 => {
                let block_end = pos
                    .checked_add(record_len + 5)
                    .filter(|&end| end <= bytes.len())
                    .ok_or("Truncated sample block while cropping")?;
                if selected.contains(&row) {
                    output.extend_from_slice(&bytes[pos..block_end]);
                }
                row += 1;
                pos = block_end;
            }
            1 => {
                pos = pos
                    .checked_add(54)
                    .filter(|&end| end <= bytes.len())
                    .ok_or("Truncated marker while cropping")?;
            }
            _ => return Err("Unsupported block encountered while cropping".into()),
        }
    }
    if row != dataset.timestamps.len() {
        return Err("Sample count changed while cropping".into());
    }
    Ok(output)
}
