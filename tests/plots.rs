use log_viewer::{log::parse, ui::plots::geometry};
#[test]
fn plot_geometry_keeps_extrema_and_breaks_at_missing_samples() {
    let mut data = parse(include_bytes!("fixtures/sanitized.mlg")).unwrap();
    let id = data.field_ids("RPM")[0];
    data.elapsed = (0..100).map(f64::from).collect();
    data.timestamps = data.elapsed.clone();
    let mut values = vec![Some(50.0); 100];
    values[20] = Some(1.0);
    values[30] = Some(99.0);
    values[50] = None;
    data.columns[id] = values;
    let segments = geometry(&data, id, 0, 100, 16, false);
    assert!(segments
        .iter()
        .any(|s| s.iter().any(|p| p.x == 20.0 && p.y == 1.0)));
    assert!(segments
        .iter()
        .any(|s| s.iter().any(|p| p.x == 30.0 && p.y == 99.0)));
    assert!(segments
        .iter()
        .all(|s| !s.iter().any(|p| p.x < 50.0) || !s.iter().any(|p| p.x > 50.0)));
    assert!(segments
        .iter()
        .flatten()
        .all(|p| data.columns[id][p.x as usize] == Some(p.y)));
}
#[test]
fn fragmented_data_keeps_global_extrema_without_joining_gaps() {
    let mut data = parse(include_bytes!("fixtures/sanitized.mlg")).unwrap();
    let id = data.field_ids("RPM")[0];
    data.elapsed = (0..200).map(f64::from).collect();
    data.timestamps = data.elapsed.clone();
    let mut values = (0..200)
        .map(|i| (i % 2 == 0).then_some(5.0))
        .collect::<Vec<_>>();
    values[180] = Some(-10.0);
    values[190] = Some(50.0);
    data.columns[id] = values;
    let segments = geometry(&data, id, 0, 200, 16, false);
    assert!(segments.iter().flatten().any(|p| p.y == -10.0));
    assert!(segments.iter().flatten().any(|p| p.y == 50.0));
    assert!(segments.iter().all(|s| s.len() == 1));
}
#[test]
fn normalization_changes_only_plot_coordinates() {
    let data = parse(include_bytes!("fixtures/sanitized.mlg")).unwrap();
    let id = data.field_ids("RPM")[0];
    let points = geometry(&data, id, 0, 3, 16, true);
    assert_eq!(points[0][0].y, 0.0);
    assert_eq!(points[0].last().unwrap().y, 1.0);
    assert_eq!(data.columns[id], [Some(1000.0), Some(1100.0), Some(1200.0)]);
}
