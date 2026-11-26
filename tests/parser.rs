use log_viewer::log::{parse, FieldKind};

fn field(kind: u8, name: &str, unit: &str) -> Vec<u8> {
    let mut f = vec![0; 89];
    f[0] = kind;
    f[1..1 + name.len()].copy_from_slice(name.as_bytes());
    f[35..35 + unit.len()].copy_from_slice(unit.as_bytes());
    f[46..50].copy_from_slice(&1f32.to_be_bytes());
    f
}
fn fixture(times: &[f32]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&[0x4d, 0x4c, 0x56, 0x4c, 0x47, 0x00, 0x00, 0x02]);
    b.extend_from_slice(&0u32.to_be_bytes());
    b.extend_from_slice(&0u32.to_be_bytes());
    b.extend_from_slice(&(202u32).to_be_bytes());
    b.extend_from_slice(&6u16.to_be_bytes());
    b.extend_from_slice(&2u16.to_be_bytes());
    b.extend(field(7, "Time", "s"));
    b.extend(field(2, "RPM", "rpm"));
    for (i, time) in times.iter().enumerate() {
        let mut row = time.to_be_bytes().to_vec();
        row.extend_from_slice(&1000u16.to_be_bytes());
        b.extend_from_slice(&[0, i as u8, 0, 0]);
        b.extend_from_slice(&row);
        b.push(row.iter().fold(0u8, |sum, x| sum.wrapping_add(*x)));
    }
    b
}
#[test]
fn rows_lookup_duplicates_and_metadata() {
    let d = parse(&fixture(&[10., 10., 11.])).unwrap();
    assert_eq!(d.field_ids("RPM"), &[1]);
    assert_eq!(d.elapsed, [0., 0., 1.]);
    assert_eq!(d.sample_at(0.), Some(1));
    assert_eq!(d.time_range(0., 0.), 0..2);
    assert_eq!(d.statistics(1).unwrap().median, 1000.);
    assert!(d.statistics(99).is_none());
    assert_eq!(d.columns[1], [Some(1000.), Some(1000.), Some(1000.)]);
    assert!(matches!(d.fields[1].kind, FieldKind::Numeric { .. }));
}

#[test]
fn crop_keeps_only_original_inclusive_rows_and_remains_parseable() {
    let original = fixture(&[10., 10., 11.]);
    let dataset = parse(&original).unwrap();
    let cropped = log_viewer::log::crop(&original, &dataset, 0.0, 0.0).unwrap();
    let result = parse(&cropped).unwrap();
    assert_eq!(result.timestamps, [10.0, 10.0]);
    assert_eq!(result.columns[result.field_ids("RPM")[0]], [Some(1000.); 2]);

    let cropped = log_viewer::log::crop(&original, &dataset, 0.5, 0.9).unwrap_err();
    assert!(cropped.contains("does not contain"));
}
#[test]
fn rejects_decreasing_bad_crc_and_ambiguous_time() {
    assert!(parse(&fixture(&[1., 0.]))
        .unwrap_err()
        .contains("decreases"));
    let mut b = fixture(&[0.]);
    *b.last_mut().unwrap() ^= 1;
    assert!(parse(&b).unwrap_err().contains("Checksum"));
    let mut b = fixture(&[]);
    b[24 + 35] = b'm';
    assert!(parse(&b).unwrap_err().contains("ambiguous"));
    assert!(parse(b"<xml/>").is_err());
}
#[test]
fn empty_and_nonfinite_values() {
    let d = parse(&fixture(&[])).unwrap();
    assert_eq!(d.sample_at(0.), None);
    assert_eq!(d.duration(), 0.);
    assert!(parse(&fixture(&[f32::NAN]))
        .unwrap_err()
        .contains("non-finite timestamp"));
}
#[test]
fn local_original_fixture_when_requested() {
    let Ok(path) = std::env::var("LOG_VIEWER_FIXTURE") else {
        return;
    };
    let bytes = std::fs::read(path).unwrap();
    let d = parse(&bytes).unwrap();
    assert_eq!(
        d.fields
            .iter()
            .filter(|f| matches!(f.provenance, log_viewer::log::Provenance::Source))
            .count(),
        74
    );
    assert_eq!(d.timestamps.len(), 23084);
    assert_eq!(d.field_ids("AFR error").len(), 1);
    assert_eq!(d.field_ids("Lambda error").len(), 1);
    assert_eq!(d.field_ids("Boost pressure").len(), 1);
    let afr = log_viewer::analysis::maps::from_source(&d, "afrTable").unwrap();
    assert_eq!(afr.ignored_duplicate_rpm, 2);
    assert_eq!(afr.x.last(), Some(&6500.0));
    let ve = log_viewer::analysis::maps::from_source(&d, "veTable").unwrap();
    assert_eq!(ve.ignored_duplicate_rpm, 1);
    assert_eq!(ve.x.last(), Some(&6600.0));
    assert!(log_viewer::analysis::maps::from_source(&d, "veTable2")
        .unwrap_err()
        .contains("Secondary fuel-load channel identity"));
    let cropped = log_viewer::log::crop(&bytes, &d, 0.0, 10.0).unwrap();
    let cropped_data = parse(&cropped).unwrap();
    assert_eq!(cropped_data.metadata.info, d.metadata.info);
    assert_eq!(
        cropped_data.metadata.tune_constants.len(),
        d.metadata.tune_constants.len()
    );
    assert_eq!(cropped_data.field_ids("AFR error (%)").len(), 1);
    assert!(log_viewer::analysis::maps::from_source(&cropped_data, "afrTable").is_ok());
    assert!(log_viewer::analysis::maps::from_source(&cropped_data, "veTable").is_ok());
    let sample = 10;
    let measured = d.columns[d.field_ids("AFR")[0]][sample].unwrap();
    let target = d.columns[d.field_ids("AFR Target")[0]][sample].unwrap();
    assert_eq!(
        d.columns[d.field_ids("AFR error")[0]][sample],
        Some(measured - target)
    );
    let map = d.columns[d.field_ids("MAP")[0]][sample].unwrap();
    let baro = d.columns[d.field_ids("Baro Pressure")[0]][sample].unwrap();
    assert_eq!(
        d.columns[d.field_ids("Boost pressure")[0]][sample],
        Some(map - baro)
    );
    assert_eq!(d.metadata.marker_count, 2);
    assert_eq!(d.field_ids("RPM").len(), 1);
    assert_eq!(d.fields[d.field_ids("AFR Target")[0]].unit, "O2");
    assert!(
        d.metadata.tune_diagnostic.is_none(),
        "{:?}",
        d.metadata.tune_diagnostic
    );
    assert!(d
        .metadata
        .tune_constants
        .iter()
        .any(|c| c.name == "veTable" && c.rows == Some(16) && c.cols == Some(16)));
}

fn with_tune(xml: &str) -> Vec<u8> {
    let mut b = fixture(&[]);
    let info = format!("NEW_INFO_PROVIDER,LogStart_MAIN_TUNE,{xml}");
    let data_start = b.len() + info.len();
    b[12..16].copy_from_slice(&202u32.to_be_bytes());
    b[16..20].copy_from_slice(&(data_start as u32).to_be_bytes());
    b.extend_from_slice(info.as_bytes());
    b
}
#[test]
fn embedded_tune_metadata_and_malformed_xml() {
    let xml = r#"<?xml version="1.0"?><msq xmlns="urn:log-viewer:test:msq"><page><constant name="veTable" rows="1" cols="1" units="%">42</constant></page></msq>"#;
    let d = parse(&with_tune(xml)).unwrap();
    assert_eq!(d.metadata.tune_constants[0].name, "veTable");
    assert_eq!(d.metadata.tune_constants[0].value, "42");
    let malformed = r#"<?xml version="1.0"?><msq><page><constant></page></msq>"#;
    assert!(parse(&with_tune(malformed))
        .unwrap()
        .metadata
        .tune_diagnostic
        .is_some());
    let dtd = r#"<?xml version="1.0"?><!DOCTYPE msq [<!ENTITY x SYSTEM "file:///etc/passwd">]><msq>&x;</msq>"#;
    assert!(parse(&with_tune(dtd))
        .unwrap()
        .metadata
        .tune_diagnostic
        .unwrap()
        .contains("forbidden"));
}

#[test]
fn sanitized_source_descriptors() {
    let d = parse(include_bytes!("fixtures/sanitized.mlg")).unwrap();
    assert_eq!(d.fields.len(), 74);
    assert_eq!(d.timestamps.len(), 3);
    assert_eq!(
        d.columns[d.field_ids("RPM")[0]],
        [Some(1000.), Some(1100.), Some(1200.)]
    );
    assert_eq!(d.fields[d.field_ids("AFR Target")[0]].unit, "O2");
    assert!(d
        .fields
        .iter()
        .any(|f| matches!(f.kind, FieldKind::Bits { .. })));
}

#[test]
fn bundled_demo_is_parseable_without_capture_or_tune_metadata() {
    let data = parse(include_bytes!("../samples/demo.mlg")).unwrap();
    assert!(!data.timestamps.is_empty());
    assert!(data.duration() > 0.0);
    assert!(data.metadata.info.is_empty());
    assert!(data.metadata.tune_constants.is_empty());
}

#[test]
fn nonfinite_channel_is_missing_and_truncated_record_is_rejected() {
    let mut b = fixture(&[0.0]);
    b[24 + 89] = 7;
    b[20..22].copy_from_slice(&8u16.to_be_bytes());
    b.truncate(202);
    let mut row = 0f32.to_be_bytes().to_vec();
    row.extend_from_slice(&f32::NAN.to_be_bytes());
    b.extend_from_slice(&[0, 0, 0, 0]);
    b.extend_from_slice(&row);
    b.push(row.iter().fold(0u8, |sum, x| sum.wrapping_add(*x)));
    let d = parse(&b).unwrap();
    assert_eq!(d.columns[1], [None]);
    b.pop();
    assert!(parse(&b).unwrap_err().contains("Truncated sample"));
}

#[test]
fn embedded_tune_encodings_are_not_guessed() {
    let utf8 = r#"<?xml version="1.0" encoding="UTF-8"?><msq><page><constant name="café">1</constant></page></msq>"#;
    let d = parse(&with_tune(utf8)).unwrap();
    assert_eq!(d.metadata.tune_constants[0].name, "café");
    let mut latin = with_tune(&utf8.replace("UTF-8", "ISO-8859-1"));
    let at = latin.windows(2).position(|w| w == [0xc3, 0xa9]).unwrap();
    latin.splice(at..at + 2, [0xe9]);
    let len = latin.len() as u32;
    latin[16..20].copy_from_slice(&len.to_be_bytes());
    let d = parse(&latin).unwrap();
    assert_eq!(d.metadata.tune_constants[0].name, "café");
    let bad = utf8.replace("UTF-8", "UTF-16");
    assert!(parse(&with_tune(&bad))
        .unwrap()
        .metadata
        .tune_diagnostic
        .unwrap()
        .contains("encoding"));
}
