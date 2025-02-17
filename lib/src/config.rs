/******************************************************************************
 * Copyright ContinuousC. Licensed under the "Elastic License 2.0".           *
 ******************************************************************************/

use std::{fmt::Display, num::ParseIntError, ops::Mul, str::FromStr};

use chrono::TimeDelta;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct WindowConfig {
    pub bin_width: Duration,
    pub num_bins: usize,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            bin_width: Duration::Seconds(30),
            num_bins: 10,
        }
    }
}

#[derive(SerializeDisplay, DeserializeFromStr, PartialEq, Eq, Clone, Copy, Debug)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schemars", schemars(with = "String"))]
pub enum Duration {
    Seconds(u32),
    Minutes(u32),
    Hours(u32),
    Days(u32),
    Weeks(u32),
}

impl Duration {
    pub const fn minutes(&self) -> f64 {
        match self {
            Duration::Seconds(n) => *n as f64 / 60.0,
            Duration::Minutes(n) => *n as f64,
            Duration::Hours(n) => *n as f64 * 60.0,
            Duration::Days(n) => *n as f64 * 24.0 * 60.0,
            Duration::Weeks(n) => *n as f64 * 7.0 * 24.0 * 60.0,
        }
    }

    pub const fn multiply(self, rhs: u32) -> Self {
        match self {
            Duration::Seconds(n) => Duration::Seconds(n * rhs),
            Duration::Minutes(n) => Duration::Minutes(n * rhs),
            Duration::Hours(n) => Duration::Hours(n * rhs),
            Duration::Days(n) => Duration::Days(n * rhs),
            Duration::Weeks(n) => Duration::Weeks(n * rhs),
        }
    }
}

impl Display for Duration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Duration::Seconds(n) => write!(f, "{n}s"),
            Duration::Minutes(n) => write!(f, "{n}m"),
            Duration::Hours(n) => write!(f, "{n}h"),
            Duration::Days(n) => write!(f, "{n}d"),
            Duration::Weeks(n) => write!(f, "{n}w"),
        }
    }
}

impl FromStr for Duration {
    type Err = ParseDurationErr;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (num, unit) = s.split_at(
            s.find(|c: char| !c.is_ascii_digit())
                .ok_or(ParseDurationErr::MissingUnit)?,
        );
        let n = num.parse().map_err(ParseDurationErr::ParseInt)?;
        match unit {
            "s" => Ok(Duration::Seconds(n)),
            "m" => Ok(Duration::Minutes(n)),
            "h" => Ok(Duration::Hours(n)),
            "d" => Ok(Duration::Days(n)),
            "w" => Ok(Duration::Weeks(n)),
            _ => Err(ParseDurationErr::InvalidUnit(unit.to_string())),
        }
    }
}

impl Mul<u32> for Duration {
    type Output = Self;

    fn mul(self, rhs: u32) -> Self::Output {
        self.multiply(rhs)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ParseDurationErr {
    #[error("invalid number: {0}")]
    ParseInt(ParseIntError),
    #[error("invalid unit: {0}")]
    InvalidUnit(String),
    #[error("missing unit")]
    MissingUnit,
}

impl From<Duration> for TimeDelta {
    fn from(val: Duration) -> Self {
        val.to_time_delta()
    }
}

impl Duration {
    pub const fn to_time_delta(self) -> TimeDelta {
        match self {
            Duration::Seconds(n) => TimeDelta::seconds(n as i64),
            Duration::Minutes(n) => TimeDelta::minutes(n as i64),
            Duration::Hours(n) => TimeDelta::hours(n as i64),
            Duration::Days(n) => TimeDelta::days(n as i64),
            Duration::Weeks(n) => TimeDelta::weeks(n as i64),
        }
    }
}
