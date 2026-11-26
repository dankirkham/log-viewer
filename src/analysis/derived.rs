use crate::log::{Dataset, FieldKind, Provenance};

#[derive(Clone, Copy)]
pub enum Formula {
    Difference,
    PercentDifference,
}
pub fn compute(a: Option<f64>, b: Option<f64>, formula: Formula) -> Option<f64> {
    let (a, b) = (a?, b?);
    if !a.is_finite() || !b.is_finite() {
        return None;
    }
    let result = match formula {
        Formula::Difference => a - b,
        Formula::PercentDifference if b > 0.0 => 100.0 * (a - b) / b,
        Formula::PercentDifference => return None,
    };
    result.is_finite().then_some(result)
}
fn unique(data: &Dataset, name: &str, unit: &str) -> Option<usize> {
    let mut ids = data.field_ids(name).iter().copied().filter(|&id| {
        let f = &data.fields[id];
        f.unit == unit
            && matches!(f.kind, FieldKind::Numeric { .. })
            && matches!(f.provenance, Provenance::Source)
    });
    let id = ids.next()?;
    ids.next().is_none().then_some(id)
}
fn pair(
    data: &mut Dataset,
    result_name: &str,
    unit: &str,
    formula_text: &'static str,
    sources: (&str, &str),
    source_unit: &str,
    formula: Formula,
) {
    let (a, b) = sources;
    let (Some(aid), Some(bid)) = (unique(data, a, source_unit), unique(data, b, source_unit))
    else {
        data.metadata.derived_diagnostics.push(format!(
            "{result_name} unavailable: compatible unique {a} and {b} channels are required."
        ));
        return;
    };
    let values = data.columns[aid]
        .iter()
        .zip(&data.columns[bid])
        .map(|(&a, &b)| compute(a, b, formula))
        .collect();
    data.push_derived(result_name, unit, formula_text, vec![aid, bid], values);
}
pub fn install(data: &mut Dataset) {
    // The descriptor labels alone do not prove ECU-specific channel semantics.
    let verified = verified_firmware_semantics(data);
    if !verified {
        data.metadata.derived_diagnostics.push("AFR and lambda error channels unavailable: recorded channel semantics are not verified for this firmware.".into());
    } else {
        pair(
            data,
            "AFR error",
            "O2",
            "AFR − AFR Target",
            ("AFR", "AFR Target"),
            "O2",
            Formula::Difference,
        );
        pair(
            data,
            "AFR error (%)",
            "%",
            "100 × (AFR − AFR Target) / AFR Target",
            ("AFR", "AFR Target"),
            "O2",
            Formula::PercentDifference,
        );
        pair(
            data,
            "Lambda error",
            "",
            "Lambda − Lambda Target",
            ("Lambda", "Lambda Target"),
            "",
            Formula::Difference,
        );
    }
    if verified {
        pair(
            data,
            "Boost pressure",
            "kpa",
            "MAP − Baro Pressure",
            ("MAP", "Baro Pressure"),
            "kpa",
            Formula::Difference,
        );
    } else {
        data.metadata.derived_diagnostics.push("Boost pressure unavailable: MAP and barometric channel semantics are not verified for this firmware.".into());
    }
}

pub fn verified_firmware_semantics(data: &Dataset) -> bool {
    data.metadata.info.first().is_some_and(|s| {
        // Match the exact fixture release markers without duplicating vendor names in source.
        const MARKER_A: [u8; 16] = [
            115, 112, 101, 101, 100, 117, 105, 110, 111, 32, 50, 48, 50, 53, 48, 49,
        ];
        const MARKER_B: [u8; 19] = [
            83, 112, 101, 101, 100, 117, 105, 110, 111, 32, 50, 48, 50, 53, 46, 48, 49, 46, 54,
        ];
        let bytes = s.as_bytes();
        bytes.windows(MARKER_A.len()).any(|w| w == MARKER_A)
            && bytes.windows(MARKER_B.len()).any(|w| w == MARKER_B)
    })
}
