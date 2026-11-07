//! Data-driven rules: symptom -> ordered list of setup changes.
//! The rules live in JSON so they can be tuned without touching code.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::*;

const BUILTIN_RULES: &str = include_str!("../rules/gt.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParamInfo {
    pub label: String,
    pub group: String,
    /// What "increase" means in plain words, e.g. "Stiffer" or "More to the front".
    pub increase: String,
    pub decrease: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorInfo {
    pub id: String,
    pub label: String,
    pub description: String,
    /// Phases the user can pick. Empty = the phase is implied.
    #[serde(default)]
    pub phases: Vec<Phase>,
    /// Ask "slow or fast corners?" for this behaviour.
    #[serde(default)]
    pub speed_matters: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    pub behavior: String,
    /// Empty = any phase.
    #[serde(default)]
    pub phases: Vec<Phase>,
    /// None = any speed. Only Slow or Fast make sense here.
    #[serde(default)]
    pub speed: Option<CornerSpeed>,
    #[serde(default)]
    pub requires_aero: bool,
    /// Empty = any drivetrain.
    #[serde(default)]
    pub drivetrains: Vec<Drivetrain>,
    /// Ordered: the first change is the one to try first.
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub param: String,
    pub dir: Direction,
    /// Clicks for a slight problem; scaled by severity up to `max_clicks`.
    #[serde(default = "one")]
    pub clicks: u32,
    #[serde(default = "three")]
    pub max_clicks: u32,
    pub why: String,
}

fn one() -> u32 {
    1
}
fn three() -> u32 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleBook {
    pub params: BTreeMap<String, ParamInfo>,
    pub behaviors: Vec<BehaviorInfo>,
    pub rules: Vec<Rule>,
}

impl RuleBook {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Rules shipped with the app (GT-style cars).
    pub fn builtin() -> Self {
        Self::from_json(BUILTIN_RULES).expect("built-in rules must parse")
    }

    /// Returns a list of problems, empty when the rule book is consistent.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        for rule in &self.rules {
            if !self.behaviors.iter().any(|b| b.id == rule.behavior) {
                errors.push(format!("rule {}: unknown behavior '{}'", rule.id, rule.behavior));
            }
            if matches!(rule.speed, Some(CornerSpeed::Both)) {
                errors.push(format!("rule {}: speed 'both' is not allowed, leave it out", rule.id));
            }
            for c in &rule.changes {
                if !self.params.contains_key(&c.param) {
                    errors.push(format!("rule {}: unknown param '{}'", rule.id, c.param));
                }
                if c.clicks == 0 || c.clicks > c.max_clicks {
                    errors.push(format!("rule {}: bad clicks for '{}'", rule.id, c.param));
                }
            }
        }
        errors
    }

    pub fn behavior(&self, id: &str) -> Option<&BehaviorInfo> {
        self.behaviors.iter().find(|b| b.id == id)
    }

    /// Short human label, e.g. "Understeer · mid-corner · slow corners".
    pub fn symptom_label(&self, s: &Symptom) -> String {
        let mut parts = vec![self.behavior(&s.behavior).map(|b| b.label.clone()).unwrap_or_else(|| s.behavior.clone())];
        if let Some(p) = s.phase {
            parts.push(
                match p {
                    Phase::Braking => "braking",
                    Phase::Entry => "corner entry",
                    Phase::Mid => "mid-corner",
                    Phase::Exit => "corner exit",
                    Phase::Straight => "straights",
                }
                .into(),
            );
        }
        let speed_matters = self.behavior(&s.behavior).is_some_and(|b| b.speed_matters);
        if speed_matters {
            match s.speed {
                CornerSpeed::Slow => parts.push("slow corners".into()),
                CornerSpeed::Fast => parts.push("fast corners".into()),
                CornerSpeed::Both => {}
            }
        }
        parts.join(" · ")
    }

    fn matches(rule: &Rule, car: &CarContext, s: &Symptom) -> bool {
        if rule.behavior != s.behavior {
            return false;
        }
        if rule.requires_aero && !car.has_aero {
            return false;
        }
        if !rule.drivetrains.is_empty() && !rule.drivetrains.contains(&car.drivetrain) {
            return false;
        }
        if !rule.phases.is_empty() {
            match s.phase {
                Some(p) if rule.phases.contains(&p) => {}
                _ => return false,
            }
        }
        match (rule.speed, s.speed) {
            (None, _) | (_, CornerSpeed::Both) => true,
            (Some(r), s) => r == s,
        }
    }

    /// Turn the driver's symptoms into a ranked list of changes.
    ///
    /// Changes to the same parameter are merged. When two symptoms pull the same
    /// parameter in opposite directions, the stronger one wins and the other is
    /// reported in `conflicts`.
    pub fn suggest(&self, car: &CarContext, symptoms: &[Symptom]) -> Vec<Suggestion> {
        self.suggest_with_history(car, symptoms, &History::default())
    }

    /// Like [`suggest`](Self::suggest), but aware of what was already changed and fixed.
    ///
    /// - A change that would reverse earlier changes is ranked down, capped at
    ///   half of what was changed, and lists those changes in `undoes`.
    /// - A change that a fixed problem's rules argue against lists that problem
    ///   in `would_worsen` and is ranked down.
    /// - A change in the same direction as earlier ones reports `already_clicks`.
    pub fn suggest_with_history(&self, car: &CarContext, symptoms: &[Symptom], history: &History) -> Vec<Suggestion> {
        let votes = self.votes(car, symptoms);
        // Fixed problems vote too, but only as a guard against undoing them.
        let fixed: Vec<Symptom> = history
            .fixed
            .iter()
            .filter(|f| !symptoms.iter().any(|s| s.same_problem(f)))
            .map(|f| Symptom { severity: Severity::Slight, ..f.clone() })
            .collect();
        let guards = self.votes(car, &fixed);

        let mut out = self.merge(votes);
        for s in &mut out {
            // Net clicks already applied to this parameter (positive = increased).
            let net: i32 = history
                .applied
                .iter()
                .filter(|a| a.param == s.param)
                .map(|a| if a.direction == Direction::Increase { a.clicks as i32 } else { -(a.clicks as i32) })
                .sum();
            let ours = if s.direction == Direction::Increase { 1 } else { -1 };
            if net * ours < 0 {
                s.undoes = history
                    .applied
                    .iter()
                    .filter(|a| a.param == s.param && a.direction != s.direction)
                    .cloned()
                    .collect();
                // Go back at most half way, and only after other options.
                s.clicks = s.clicks.min(net.unsigned_abs().div_ceil(2).max(1));
                s.score *= 0.35;
            } else if net != 0 {
                s.already_clicks = net.unsigned_abs();
                // Diminishing returns: prefer spreading changes over many parameters.
                s.score *= 0.85;
            }
            if let Some((weight, _, reasons)) = guards.get(&(s.param.clone(), s.direction.opposite())) {
                s.would_worsen = reasons.clone();
                s.score -= weight * 0.5;
            }
        }
        out.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.label.cmp(&b.label)));
        out
    }

    /// (param, direction) -> (score, clicks, reasons)
    fn votes(&self, car: &CarContext, symptoms: &[Symptom]) -> BTreeMap<(String, Direction), (f32, u32, Vec<Reason>)> {
        let mut votes: BTreeMap<(String, Direction), (f32, u32, Vec<Reason>)> = BTreeMap::new();
        for s in symptoms {
            let label = self.symptom_label(s);
            let mult = s.severity.multiplier();
            for rule in self.rules.iter().filter(|r| Self::matches(r, car, s)) {
                for (rank, c) in rule.changes.iter().enumerate() {
                    let weight = (1.0 - 0.15 * rank as f32).max(0.3) * mult as f32;
                    let clicks = (c.clicks * mult).min(c.max_clicks);
                    let entry = votes.entry((c.param.clone(), c.dir)).or_insert((0.0, 0, Vec::new()));
                    entry.0 += weight;
                    entry.1 = entry.1.max(clicks);
                    if !entry.2.iter().any(|r| r.symptom == label && r.why == c.why) {
                        entry.2.push(Reason { symptom: label.clone(), why: c.why.clone() });
                    }
                }
            }
        }
        votes
    }

    /// One suggestion per parameter; opposite votes become `conflicts`.
    fn merge(&self, votes: BTreeMap<(String, Direction), (f32, u32, Vec<Reason>)>) -> Vec<Suggestion> {
        let mut out: Vec<Suggestion> = Vec::new();
        let params: Vec<String> = {
            let mut p: Vec<String> = votes.keys().map(|(p, _)| p.clone()).collect();
            p.dedup();
            p
        };
        for param in params {
            let inc = votes.get(&(param.clone(), Direction::Increase));
            let dec = votes.get(&(param.clone(), Direction::Decrease));
            let (dir, win, lose) = match (inc, dec) {
                (Some(i), Some(d)) if i.0 >= d.0 => (Direction::Increase, i, Some(d)),
                (Some(i), Some(d)) => (Direction::Decrease, d, Some(i)),
                (Some(i), None) => (Direction::Increase, i, None),
                (None, Some(d)) => (Direction::Decrease, d, None),
                (None, None) => continue,
            };
            let info = &self.params[&param];
            out.push(Suggestion {
                label: info.label.clone(),
                group: info.group.clone(),
                param,
                direction: dir,
                clicks: win.1,
                reasons: win.2.clone(),
                conflicts: lose.map(|l| l.2.clone()).unwrap_or_default(),
                // A conflict lowers confidence in the change.
                score: win.0 - lose.map(|l| l.0).unwrap_or(0.0),
                undoes: Vec::new(),
                already_clicks: 0,
                would_worsen: Vec::new(),
            });
        }
        out.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.label.cmp(&b.label)));
        out
    }
}
