//! User-facing file versioning choices <-> Syncthing's `versioning` object.
//!
//! Units verified against Syncthing's source (lib/versioner): staggered
//! `maxAge` is in **seconds** (default 31536000 = 365 days), trashcan and
//! simple `cleanoutDays` are days, simple `keep` is a count. All params are
//! strings. Archived files live in `.stversions/` as `name~YYYYMMDD-HHMMSS.ext`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::client::Versioning;

const DAY: u64 = 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Choice {
    /// No versioning: replaced/deleted files are gone.
    Off,
    /// Keeps the last deleted/replaced copy; cleans after `days` (0 = never).
    Trashcan { days: u32 },
    /// Keeps the `keep` most recent copies; cleans after `days` (0 = never).
    Simple { keep: u32, days: u32 },
    /// Thins out copies over time and keeps them for `days` (0 = forever).
    Staggered { days: u32 },
}

fn num(v: &Versioning, k: &str) -> Option<u64> {
    v.params.get(k).and_then(|s| s.trim().parse().ok())
}

fn clamp(n: u64) -> u32 {
    n.min(u64::from(u32::MAX)) as u32
}

impl Choice {
    /// Reads the current config. Unknown types (e.g. `external`) map to
    /// `None` so the UI shows them as "custom" and never overwrites blindly.
    pub fn from_versioning(v: &Versioning) -> Option<Choice> {
        match v.kind.as_str() {
            "" | "none" => Some(Choice::Off),
            "trashcan" => Some(Choice::Trashcan {
                days: clamp(num(v, "cleanoutDays").unwrap_or(0)),
            }),
            "simple" => Some(Choice::Simple {
                keep: clamp(num(v, "keep").unwrap_or(5)),
                days: clamp(num(v, "cleanoutDays").unwrap_or(0)),
            }),
            "staggered" => Some(Choice::Staggered {
                // Round up so a 1-second remainder is not shown as 0 (forever).
                days: clamp(num(v, "maxAge").unwrap_or(365 * DAY).div_ceil(DAY)),
            }),
            _ => None,
        }
    }

    /// Builds the new object, keeping unrelated fields of `current`
    /// (`cleanupIntervalS`, `fsPath`, `fsType`) since a PATCH of
    /// `versioning` replaces the whole object.
    pub fn apply(self, current: &Versioning) -> Versioning {
        let mut params = BTreeMap::new();
        let kind = match self {
            Choice::Off => "",
            Choice::Trashcan { days } => {
                params.insert("cleanoutDays".into(), days.to_string());
                "trashcan"
            }
            Choice::Simple { keep, days } => {
                params.insert("keep".into(), keep.max(1).to_string());
                params.insert("cleanoutDays".into(), days.to_string());
                "simple"
            }
            Choice::Staggered { days } => {
                params.insert("maxAge".into(), (u64::from(days) * DAY).to_string());
                "staggered"
            }
        };
        Versioning {
            kind: kind.into(),
            params,
            extra: current.extra.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn v(j: serde_json::Value) -> Versioning {
        serde_json::from_value(j).unwrap()
    }

    #[test]
    fn staggered_max_age_is_seconds() {
        let out = Choice::Staggered { days: 30 }.apply(&Versioning::default());
        assert_eq!(out.kind, "staggered");
        assert_eq!(out.params["maxAge"], "2592000");
        assert_eq!(out, Versioning::staggered_days(30));
        assert_eq!(
            Choice::from_versioning(&out),
            Some(Choice::Staggered { days: 30 })
        );
    }

    #[test]
    fn reads_syncthing_defaults_and_rounds_up() {
        let x = v(json!({"type":"staggered","params":{"maxAge":"31536000"}}));
        assert_eq!(
            Choice::from_versioning(&x),
            Some(Choice::Staggered { days: 365 })
        );
        let x = v(json!({"type":"staggered","params":{"maxAge":"86401"}}));
        assert_eq!(
            Choice::from_versioning(&x),
            Some(Choice::Staggered { days: 2 })
        );
        let x = v(json!({"type":"staggered","params":{"maxAge":"0"}}));
        assert_eq!(
            Choice::from_versioning(&x),
            Some(Choice::Staggered { days: 0 })
        );
    }

    #[test]
    fn trashcan_simple_and_off_roundtrip() {
        for c in [
            Choice::Off,
            Choice::Trashcan { days: 0 },
            Choice::Trashcan { days: 14 },
            Choice::Simple { keep: 10, days: 90 },
        ] {
            assert_eq!(
                Choice::from_versioning(&c.apply(&Versioning::default())),
                Some(c)
            );
        }
        assert_eq!(Choice::Off.apply(&Versioning::default()).kind, "");
    }

    #[test]
    fn simple_keep_is_at_least_one() {
        let out = Choice::Simple { keep: 0, days: 0 }.apply(&Versioning::default());
        assert_eq!(out.params["keep"], "1");
    }

    #[test]
    fn unknown_type_is_not_mapped() {
        let x = v(json!({"type":"external","params":{"command":"x"}}));
        assert_eq!(Choice::from_versioning(&x), None);
    }

    #[test]
    fn apply_preserves_other_fields_and_drops_stale_params() {
        let cur = v(json!({
            "type":"simple","params":{"keep":"5","cleanoutDays":"0"},
            "cleanupIntervalS":3600,"fsPath":"","fsType":"basic"
        }));
        let out = Choice::Staggered { days: 7 }.apply(&cur);
        let j = serde_json::to_value(&out).unwrap();
        assert_eq!(j["cleanupIntervalS"], 3600);
        assert_eq!(j["fsType"], "basic");
        assert_eq!(j["params"], json!({"maxAge":"604800"}));
    }

    #[test]
    fn serializes_for_the_ui() {
        assert_eq!(
            serde_json::to_value(Choice::Simple { keep: 3, days: 1 }).unwrap(),
            json!({"kind":"simple","keep":3,"days":1})
        );
        assert_eq!(
            serde_json::to_value(Choice::Off).unwrap(),
            json!({"kind":"off"})
        );
    }
}
