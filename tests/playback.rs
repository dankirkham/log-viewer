use log_viewer::{log::parse, playback::Playback};
fn data() -> log_viewer::log::Dataset {
    parse(include_bytes!("fixtures/sanitized.mlg")).unwrap()
}
#[test]
fn recorded_samples_and_seek_clamping() {
    let data = data();
    let mut p = Playback::default();
    p.reset(&data);
    assert_eq!(p.sample_index, Some(0));
    p.seek(0.08, &data);
    assert_eq!(p.sample_index, Some(1));
    p.seek(-10., &data);
    assert_eq!(p.position, 0.);
    assert_eq!(p.sample_index, Some(0));
    p.seek(99., &data);
    assert_eq!(p.position, data.duration());
    assert_eq!(p.sample_index, Some(2));
    p.seek(0.08, &data);
    assert_eq!(p.sample_index, Some(1));
}
#[test]
fn speed_pause_seek_and_end_behavior() {
    let data = data();
    let mut p = Playback::default();
    p.reset(&data);
    p.set_speed(2.0);
    p.toggle(&data);
    p.tick(10., &data);
    p.tick(10.025, &data);
    assert!((p.position - 0.05).abs() < 1e-9);
    assert_eq!(p.sample_index, Some(0));
    p.seek(0.08, &data);
    assert!(p.playing);
    p.tick(100., &data);
    p.tick(101., &data);
    assert!(!p.playing);
    assert_eq!(p.position, data.duration());
    p.toggle(&data);
    assert!(p.playing);
    assert_eq!(p.position, 0.0);
    p.toggle(&data);
    let previous = p.position;
    p.tick(500., &data);
    assert_eq!(p.position, previous);
    p.set_speed(3.0);
    assert_eq!(p.speed, 2.0);
}
#[test]
fn empty_and_single_sample_are_safe() {
    let original = include_bytes!("fixtures/sanitized.mlg");
    let start = u32::from_be_bytes(original[16..20].try_into().unwrap()) as usize;
    let empty = parse(&original[..start]).unwrap();
    let one = parse(&original[..start + 163]).unwrap();
    for data in [&empty, &one] {
        let mut p = Playback::default();
        p.reset(data);
        p.toggle(data);
        p.tick(1., data);
        assert!(!p.playing);
        assert_eq!(p.position, 0.0);
        p.seek(10., data);
        assert_eq!(
            p.sample_index,
            if data.timestamps.is_empty() {
                None
            } else {
                Some(0)
            }
        );
    }
}
#[test]
fn duplicate_timestamps_select_the_last_recorded_sample() {
    let mut bytes = include_bytes!("fixtures/sanitized.mlg").to_vec();
    let start = u32::from_be_bytes(bytes[16..20].try_into().unwrap()) as usize;
    let second = start + 163;
    bytes[second + 4..second + 8].copy_from_slice(&0.0f32.to_be_bytes());
    bytes[second + 162] = bytes[second + 4..second + 162]
        .iter()
        .fold(0u8, |sum, &b| sum.wrapping_add(b));
    let data = parse(&bytes).unwrap();
    let mut p = Playback::default();
    p.reset(&data);
    assert_eq!(p.sample_index, Some(0));
    p.seek(0.0, &data);
    assert_eq!(p.sample_index, Some(1));
    p.seek(data.duration(), &data);
    p.seek(0.0, &data);
    assert_eq!(p.sample_index, Some(1));
}
#[test]
fn every_supported_speed_advances_by_wall_time() {
    let data = data();
    for speed in log_viewer::playback::SPEEDS {
        let mut p = Playback::default();
        p.reset(&data);
        p.set_speed(speed);
        p.toggle(&data);
        p.tick(4.0, &data);
        p.tick(4.001, &data);
        assert!((p.position - 0.001 * speed).abs() < 1e-10);
    }
}
