use std::str::FromStr;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Copy, ValueEnum)]
pub enum Priority {
    High,
    Medium,
    #[default]
    Low,
}
impl FromStr for Priority {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse::<i16>() {
            Ok(0) => Ok(Priority::High),
            Ok(1) => Ok(Priority::Medium),
            Ok(2) => Ok(Priority::Low),
            _ => Err(format!("invalid priority value: {}", s)),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Copy, ValueEnum)]
pub enum Day {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}
impl FromStr for Day {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse::<i16>() {
            Ok(1) => Ok(Day::Monday),
            Ok(2) => Ok(Day::Tuesday),
            Ok(3) => Ok(Day::Wednesday),
            Ok(4) => Ok(Day::Thursday),
            Ok(5) => Ok(Day::Friday),
            Ok(6) => Ok(Day::Saturday),
            Ok(0) => Ok(Day::Sunday),
            _ => Err(format!("invalid priority value: {}", s)),
        }
    }
}
impl Day {
    pub fn get_digit(&self) -> i8 {
        match self {
            Day::Monday => 1,
            Day::Tuesday => 2,
            Day::Wednesday => 3,
            Day::Thursday => 4,
            Day::Friday => 5,
            Day::Saturday => 6,
            Day::Sunday => 7,
        }
    }
}
