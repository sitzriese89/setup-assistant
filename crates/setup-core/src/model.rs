//! Types that describe the car and what the driver feels. No game knowledge here.

use serde::{Deserialize, Serialize};

/// Where in the corner the problem happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Braking,
    Entry,
    Mid,
    Exit,
    Straight,
}

/// Corner speed. Fast corners are dominated by aero, slow ones by mechanical grip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CornerSpeed {
    Slow,
    Fast,
    /// Happens everywhere, or the driver isn't sure.
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Slight,
    Moderate,
    Severe,
}

impl Severity {
    pub fn multiplier(self) -> u32 {
        match self {
            Severity::Slight => 1,
            Severity::Moderate => 2,
            Severity::Severe => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Drivetrain {
    Rwd,
    Fwd,
    Awd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Increase,
    Decrease,
}

impl Direction {
    pub fn opposite(self) -> Self {
        match self {
            Direction::Increase => Direction::Decrease,
            Direction::Decrease => Direction::Increase,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarContext {
    pub drivetrain: Drivetrain,
    /// GT3, prototypes etc. Road cars and old touring cars usually have little aero.
    pub has_aero: bool,
}

/// One problem the driver describes, e.g. "moderate mid-corner understeer in slow corners".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symptom {
    /// Behaviour id from the rules file, e.g. "understeer".
    pub behavior: String,
    /// None for behaviours where the phase is implied (lockups, bottoming...).
    pub phase: Option<Phase>,
    pub speed: CornerSpeed,
    pub severity: Severity,
}

/// A single suggested change, merged across all symptoms.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub param: String,
    pub label: String,
    pub group: String,
    pub direction: Direction,
    pub clicks: u32,
    /// Why this helps, one entry per symptom that asked for it.
    pub reasons: Vec<Reason>,
    /// Other symptoms that ask for the opposite change.
    pub conflicts: Vec<Reason>,
    /// Higher = try this first.
    pub score: f32,
    /// Earlier changes this would reverse.
    #[serde(default)]
    pub undoes: Vec<AppliedChange>,
    /// Clicks already changed in this same direction in earlier runs.
    #[serde(default)]
    pub already_clicks: u32,
    /// Problems fixed earlier that this change could bring back.
    #[serde(default)]
    pub would_worsen: Vec<Reason>,
}

/// A setup change the driver made in an earlier run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedChange {
    pub param: String,
    pub direction: Direction,
    pub clicks: u32,
    /// Labels of the problems it was meant to fix.
    #[serde(default)]
    pub for_symptoms: Vec<String>,
    #[serde(default)]
    pub run: u32,
}

/// What happened earlier in this session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct History {
    #[serde(default)]
    pub applied: Vec<AppliedChange>,
    /// Problems that went away after changes.
    #[serde(default)]
    pub fixed: Vec<Symptom>,
}

impl Symptom {
    /// Same behaviour in the same place, ignoring severity.
    pub fn same_problem(&self, other: &Symptom) -> bool {
        self.behavior == other.behavior && self.phase == other.phase && self.speed == other.speed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reason {
    pub symptom: String,
    pub why: String,
}
