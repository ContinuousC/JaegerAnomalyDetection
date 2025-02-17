/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use std::{fmt::Display, str::FromStr};

use prometheus_core::LabelName;
use prometheus_expr::LabelSelector;
use serde_with::{DeserializeFromStr, SerializeDisplay};

use crate::{Duration, WindowConfig};

#[derive(
    SerializeDisplay, DeserializeFromStr, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug,
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub enum Interval {
    Immediate(ImmediateInterval),
    Reference(ReferenceInterval),
}

impl Interval {
    pub(crate) fn labels(self) -> impl Iterator<Item = (LabelName, LabelSelector)> {
        std::iter::once(self.label())
    }

    fn label(self) -> (LabelName, LabelSelector) {
        match self {
            Interval::Immediate(immediate_interval) => immediate_interval.label(),
            Interval::Reference(reference_interval) => reference_interval.label(),
        }
    }
}

impl From<ImmediateInterval> for Interval {
    fn from(value: ImmediateInterval) -> Self {
        Self::Immediate(value)
    }
}

impl From<ReferenceInterval> for Interval {
    fn from(value: ReferenceInterval) -> Self {
        Self::Reference(value)
    }
}

impl Display for Interval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Interval::Immediate(immediate_interval) => write!(f, "{immediate_interval}"),
            Interval::Reference(reference_interval) => write!(f, "{reference_interval}"),
        }
    }
}

impl FromStr for Interval {
    type Err = InvalidInterval;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse()
            .map(Interval::Immediate)
            .or_else(|_| s.parse().map(Interval::Reference))
            .map_err(|_| InvalidInterval)
    }
}

#[derive(thiserror::Error, Debug)]
#[error("expected '5m', '15m', '7d' or '30d'")]
pub struct InvalidInterval;

#[derive(
    SerializeDisplay, DeserializeFromStr, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug,
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub enum ReferenceInterval {
    #[cfg_attr(any(feature = "schemars", feature = "tsify"), serde(rename = "7d"))]
    R7d,
    #[cfg_attr(any(feature = "schemars", feature = "tsify"), serde(rename = "30d"))]
    R30d,
}

impl ReferenceInterval {
    pub fn window_config(self) -> WindowConfig {
        match self {
            ReferenceInterval::R7d => WindowConfig {
                bin_width: Duration::Minutes(15),
                num_bins: 7 * 24 * 4,
            },
            ReferenceInterval::R30d => WindowConfig {
                bin_width: Duration::Hours(1),
                num_bins: 30 * 24,
            },
        }
    }

    pub(crate) fn labels(self) -> impl Iterator<Item = (LabelName, LabelSelector)> {
        std::iter::once(self.label())
    }

    fn label(self) -> (LabelName, LabelSelector) {
        (
            LabelName::new_static("reference"),
            LabelSelector::Eq(self.to_string()),
        )
    }
}

impl Display for ReferenceInterval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReferenceInterval::R7d => write!(f, "7d"),
            ReferenceInterval::R30d => write!(f, "30d"),
        }
    }
}

impl FromStr for ReferenceInterval {
    type Err = InvalidReferenceInterval;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "7d" => Ok(Self::R7d),
            "30d" => Ok(Self::R30d),
            _ => Err(InvalidReferenceInterval),
        }
    }
}

#[derive(thiserror::Error, Debug)]
#[error("expected '7d' or '30d'")]
pub struct InvalidReferenceInterval;

#[derive(
    SerializeDisplay, DeserializeFromStr, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug,
)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub enum ImmediateInterval {
    #[cfg_attr(any(feature = "schemars", feature = "tsify"), serde(rename = "5m"))]
    I5m,
    #[cfg_attr(any(feature = "schemars", feature = "tsify"), serde(rename = "15m"))]
    I15m,
}

impl ImmediateInterval {
    pub fn window_config(self) -> WindowConfig {
        match self {
            ImmediateInterval::I5m => WindowConfig {
                bin_width: Duration::Seconds(30),
                num_bins: 5 * 2,
            },
            ImmediateInterval::I15m => WindowConfig {
                bin_width: Duration::Seconds(30),
                num_bins: 15 * 2,
            },
        }
    }

    pub(crate) fn labels(self) -> impl Iterator<Item = (LabelName, LabelSelector)> {
        std::iter::once(self.label())
    }

    fn label(self) -> (LabelName, LabelSelector) {
        (
            LabelName::new_static("immediate"),
            LabelSelector::Eq(self.to_string()),
        )
    }
}

impl Display for ImmediateInterval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::I5m => write!(f, "5m"),
            Self::I15m => write!(f, "15m"),
        }
    }
}

impl FromStr for ImmediateInterval {
    type Err = InvalidImmediateInterval;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "5m" => Ok(Self::I5m),
            "15m" => Ok(Self::I15m),
            _ => Err(InvalidImmediateInterval),
        }
    }
}

#[derive(thiserror::Error, Debug)]
#[error("expected '5m' or '15m'")]
pub struct InvalidImmediateInterval;
