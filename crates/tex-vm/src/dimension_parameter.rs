use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DimensionParameterId {
    HangIndent,
}

impl DimensionParameterId {
    pub const SNAPSHOT_V1_ALLOWED_IDS: &'static [Self] = &[Self::HangIndent];

    pub(crate) fn from_snapshot_command_v1_name(name: &str) -> Option<Self> {
        match name {
            "hangindent" => Some(Self::HangIndent),
            _ => None,
        }
    }

    const fn default_value(self) -> RawDimensionSp {
        match self {
            Self::HangIndent => RawDimensionSp::new(0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RawDimensionSp(i32);

impl RawDimensionSp {
    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> i32 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmDimensionParameterAssignmentV1 {
    pub parameter: DimensionParameterId,
    pub value: RawDimensionSp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VmDimensionParameterStateV1 {
    pub layers: Vec<Vec<VmDimensionParameterAssignmentV1>>,
}

impl VmDimensionParameterStateV1 {
    pub(crate) fn validate(&self, expected_layers: usize) -> Result<(), String> {
        if self.layers.len() != expected_layers {
            return Err(format!(
                "dimension-parameter layer count {} does not match VM scope depth {expected_layers}",
                self.layers.len()
            ));
        }
        if !self.layers.iter().any(|layer| !layer.is_empty()) {
            return Err("dimension-parameter state must not be empty".to_string());
        }
        if self.layers[0]
            .iter()
            .any(|assignment| assignment.value == assignment.parameter.default_value())
        {
            return Err(
                "dimension-parameter root assignments equal to their defaults must be omitted"
                    .to_string(),
            );
        }
        for (layer_index, layer) in self.layers.iter().enumerate() {
            let mut previous = None;
            for assignment in layer {
                if !DimensionParameterId::SNAPSHOT_V1_ALLOWED_IDS.contains(&assignment.parameter) {
                    return Err(format!(
                        "dimension-parameter ID {:?} is outside the V1 allowlist",
                        assignment.parameter
                    ));
                }
                if previous.is_some_and(|parameter| parameter >= assignment.parameter) {
                    return Err(format!(
                        "dimension-parameter layer {layer_index} entries must be strictly increasing"
                    ));
                }
                previous = Some(assignment.parameter);
            }
        }
        Ok(())
    }
}
