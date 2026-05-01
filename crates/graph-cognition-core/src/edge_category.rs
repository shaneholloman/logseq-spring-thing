use serde::{Deserialize, Serialize};
use strum::{EnumIter, EnumString, Display, AsRefStr};

/// 8 edge categories per ADR-064, grouping the 35 EdgeKinds.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize,
    EnumIter, EnumString, Display, AsRefStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum EdgeCategory {
    Structural,
    Behavioral,
    DataFlow,
    Dependencies,
    Semantic,
    Infrastructure,
    Domain,
    Knowledge,
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    #[test]
    fn total_categories_is_8() {
        assert_eq!(EdgeCategory::iter().count(), 8);
    }
}
