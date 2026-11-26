use log_viewer::{
    log::parse,
    ui::gauges::{Gauge, Status},
};
#[test]
fn voltage_preset_and_missing_status() {
    let data = parse(include_bytes!("fixtures/sanitized.mlg")).unwrap();
    let id = data.field_ids("Battery V")[0];
    let gauge = Gauge::from_field(&data, id).unwrap();
    assert_eq!(gauge.status(Some(13.5)), Status::Nominal);
    assert_eq!(gauge.status(Some(18.0)), Status::Critical);
    assert_eq!(gauge.status(None), Status::Missing);
    assert_eq!(gauge.status(Some(f64::NAN)), Status::Missing);
}
#[test]
fn unknown_fields_start_neutral_and_bitfields_are_not_gauges() {
    let data = parse(include_bytes!("fixtures/sanitized.mlg")).unwrap();
    let gauge = Gauge::from_field(&data, data.field_ids("RPM")[0]).unwrap();
    assert_eq!(gauge.status(Some(1000.0)), Status::Neutral);
    let bit = data
        .fields
        .iter()
        .find(|f| matches!(f.kind, log_viewer::log::FieldKind::Bits { .. }))
        .unwrap();
    assert!(Gauge::from_field(&data, bit.id).is_none());
}
