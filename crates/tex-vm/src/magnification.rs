use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequestedMagnification(i32);

impl RequestedMagnification {
    pub const DEFAULT: Self = Self(1_000);

    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> i32 {
        self.0
    }
}

impl Default for RequestedMagnification {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct PreparedMagnification(u16);

impl PreparedMagnification {
    pub const MAX: u16 = 32_768;

    pub const fn get(self) -> u16 {
        self.0
    }

    pub(crate) fn from_requested(requested: RequestedMagnification) -> Option<Self> {
        let value = u16::try_from(requested.get()).ok()?;
        (value != 0 && value <= Self::MAX).then_some(Self(value))
    }
}

impl<'de> Deserialize<'de> for PreparedMagnification {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u16::deserialize(deserializer)?;
        Self::from_requested(RequestedMagnification::new(i32::from(value)))
            .ok_or_else(|| serde::de::Error::custom("prepared magnification must be in 1..=32768"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MagnificationPreparationIssue {
    Illegal {
        requested: RequestedMagnification,
    },
    Incompatible {
        requested: RequestedMagnification,
        prepared: PreparedMagnification,
    },
}

#[cfg(test)]
mod tests {
    use super::PreparedMagnification;

    #[test]
    fn prepared_magnification_deserialization_preserves_the_legal_range() {
        assert_eq!(
            serde_json::from_str::<PreparedMagnification>("32768")
                .expect("maximum prepared magnification")
                .get(),
            32_768
        );
        for invalid in ["0", "32769"] {
            assert!(
                serde_json::from_str::<PreparedMagnification>(invalid).is_err(),
                "{invalid} must not construct a prepared magnification"
            );
        }
    }
}
