//! Wire representation of the existing preparation limits.
use openbindings::PreparationLimits;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

// Remote derives keep serialization policy in the private bridge.
#[derive(Serialize, Deserialize)]
#[serde(remote = "PreparationLimits", default, deny_unknown_fields)]
struct PreparationLimitsWire {
    max_schema_nodes: usize,
    max_identity_bytes: usize,
    max_sdk_work_units: Option<usize>,
    max_sdk_reservation_bytes: usize,
}
impl Default for PreparationLimitsWire {
    fn default() -> Self {
        let limits = PreparationLimits::default();
        Self {
            max_schema_nodes: limits.max_schema_nodes,
            max_identity_bytes: limits.max_identity_bytes,
            max_sdk_work_units: limits.max_sdk_work_units,
            max_sdk_reservation_bytes: limits.max_sdk_reservation_bytes,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(transparent)]
struct Settings(#[serde(with = "PreparationLimitsWire")] PreparationLimits);

pub(super) fn preparation(text: Option<String>) -> Result<PreparationLimits, JsValue> {
    text.map_or(Ok(PreparationLimits::default()), |text| {
        serde_json::from_str::<Settings>(&text)
            .map(|settings| settings.0)
            .map_err(|_| {
                super::error(
                    "invalid-preparation-limits",
                    "expected valid preparation limit settings",
                )
            })
    })
}

#[wasm_bindgen(js_name = preparationLimits)]
pub fn preparation_limits() -> String {
    super::encoded(Settings(PreparationLimits::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_wire_limits_use_rust_defaults_and_null_work() {
        let empty: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.0, PreparationLimits::default());
        let settings: Settings = serde_json::from_str(
            r#"{"max_schema_nodes":11,"max_identity_bytes":17,"max_sdk_work_units":0,"max_sdk_reservation_bytes":23}"#,
        ).unwrap();
        assert_eq!(
            settings.0,
            PreparationLimits {
                max_schema_nodes: 11,
                max_identity_bytes: 17,
                max_sdk_work_units: Some(0),
                max_sdk_reservation_bytes: 23,
            }
        );
        let settings: Settings = serde_json::from_str(r#"{"max_sdk_work_units":null}"#).unwrap();
        assert_eq!(settings.0, PreparationLimits::default());
        for invalid in [
            r#"{"unknown":1}"#,
            r#"{"max_schema_nodes":-1}"#,
            r#"{"max_sdk_work_units":0.5}"#,
            r#"{"max_identity_bytes":null}"#,
        ] {
            assert!(serde_json::from_str::<Settings>(invalid).is_err());
        }
    }
}
