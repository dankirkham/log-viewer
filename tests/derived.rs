use log_viewer::analysis::derived::{compute, Formula};
#[test]
fn signs_missing_and_zero_denominators() {
    assert_eq!(
        compute(Some(15.0), Some(14.0), Formula::Difference),
        Some(1.0)
    );
    assert_eq!(
        compute(Some(13.0), Some(14.0), Formula::Difference),
        Some(-1.0)
    );
    assert_eq!(
        compute(Some(15.0), Some(0.0), Formula::PercentDifference),
        None
    );
    assert_eq!(
        compute(Some(15.0), Some(-1.0), Formula::PercentDifference),
        None
    );
    assert_eq!(compute(None, Some(14.0), Formula::Difference), None);
    assert_eq!(
        compute(Some(f64::NAN), Some(14.0), Formula::Difference),
        None
    );
    assert_eq!(
        compute(Some(15.0), Some(10.0), Formula::PercentDifference),
        Some(50.0)
    );
}
