use log_viewer::{
    analysis::maps::{axis_coordinate, trail, Table},
    log::parse,
};
fn identified_firmware_info() -> String {
    String::from_utf8(vec![
        115, 112, 101, 101, 100, 117, 105, 110, 111, 32, 50, 48, 50, 53, 48, 49, 58, 32, 83, 112,
        101, 101, 100, 117, 105, 110, 111, 32, 50, 48, 50, 53, 46, 48, 49, 46, 54,
    ])
    .unwrap()
}
fn table() -> Table {
    Table {
        name: "test".into(),
        x: vec![1000.0, 1100.0, 1300.0],
        y: vec![0.0, 1.0],
        cells: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
        unit: "%".into(),
        rpm_id: 2,
        load_id: 3,
        ignored_duplicate_rpm: 0,
    }
}
#[test]
fn nonuniform_breakpoints_and_invalid_tables() {
    let mut table = table();
    assert!(table.validate().is_ok());
    assert_eq!(axis_coordinate(&table.x, 1200.0), Some(1.5));
    assert_eq!(table.position(1300.0, 0.5), Some((2.0, 0.5)));
    assert_eq!(table.position(999.0, 0.5), None);
    table.x[2] = 1100.0;
    assert!(table
        .validate()
        .unwrap_err()
        .contains("strictly increasing"));
    assert_eq!(axis_coordinate(&table.x, 1100.0), None);
    let mut invalid = table.clone();
    invalid.x[2] = 1300.0;
    invalid.cells.pop();
    assert!(invalid.validate().unwrap_err().contains("dimensions"));
}
#[test]
fn trail_reconstructs_after_backward_seek_and_missing_values() {
    let mut data = parse(include_bytes!("fixtures/sanitized.mlg")).unwrap();
    let table = table();
    let last_time = data.elapsed[2];
    let later = trail(&table, &data, last_time, Some(2), 0.1, 100);
    assert_eq!(later.len(), 2);
    assert!(later[0].opacity < later[1].opacity);
    let earlier = trail(&table, &data, data.elapsed[1], Some(1), 1.0, 100);
    assert_eq!(earlier.len(), 2);
    assert_eq!(
        earlier,
        trail(&table, &data, data.elapsed[1], Some(1), 1.0, 100)
    );
    data.columns[table.load_id][0] = None;
    assert_eq!(
        trail(&table, &data, data.elapsed[1], Some(1), 1.0, 100).len(),
        1
    );
}
#[test]
fn missing_source_table_is_unavailable() {
    let data = parse(include_bytes!("fixtures/sanitized.mlg")).unwrap();
    assert!(log_viewer::analysis::maps::from_source(&data, "veTable").is_err());
}
#[test]
fn verified_source_arrays_produce_a_table_without_inventing_values() {
    use log_viewer::log::SourceConstant;
    let mut data = parse(include_bytes!("fixtures/sanitized.mlg")).unwrap();
    data.metadata.info = vec![identified_firmware_info()];
    let constant = |name: &str, unit: &str, rows: usize, cols: usize, value: &str| SourceConstant {
        name: name.into(),
        unit: Some(unit.into()),
        rows: Some(rows),
        cols: Some(cols),
        value: value.into(),
    };
    data.metadata.tune_constants = vec![
        constant("rpmBins", "RPM", 3, 1, "1000 1100 1300"),
        constant("fuelLoadBins", "kPa", 2, 1, "20 80"),
        constant("veTable", "%", 2, 3, "1 2 3 4 5 6"),
    ];
    let t = log_viewer::analysis::maps::from_source(&data, "veTable").unwrap();
    assert_eq!(t.cells, [1., 2., 3., 4., 5., 6.]);
    assert_eq!(t.position(1200., 50.), Some((1.5, 0.5)));
    assert_eq!(data.fields[t.load_id].name, "FuelLoad");
}

#[test]
fn source_maps_keep_first_column_for_duplicate_rpm_breakpoints() {
    use log_viewer::log::SourceConstant;
    let mut data = parse(include_bytes!("fixtures/sanitized.mlg")).unwrap();
    data.metadata.info = vec![identified_firmware_info()];
    let constant = |name: &str, unit: &str, rows: usize, cols: usize, value: &str| SourceConstant {
        name: name.into(),
        unit: Some(unit.into()),
        rows: Some(rows),
        cols: Some(cols),
        value: value.into(),
    };
    data.metadata.tune_constants = vec![
        constant("rpmBins", "RPM", 4, 1, "1000 1200 1200 1400"),
        constant("fuelLoadBins", "kPa", 2, 1, "20 80"),
        constant("veTable", "%", 2, 4, "1 2 99 4 5 6 88 8"),
    ];
    let table = log_viewer::analysis::maps::from_source(&data, "veTable").unwrap();
    assert_eq!(table.x, [1000., 1200., 1400.]);
    assert_eq!(table.cells, [1., 2., 4., 5., 6., 8.]);
    assert_eq!(table.ignored_duplicate_rpm, 1);
    assert_eq!(table.position(1200., 50.), Some((1., 0.5)));
}
