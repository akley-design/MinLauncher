//! The flags of the JVM, and what the measurement said about them.
//!
//! The version of the game prescribes the flags of its virtual machine,
//! and the launcher passes them on as they are. Some of them are worth
//! a second look: the modern versions prescribe `-XX:+AlwaysPreTouch`,
//! which makes the virtual machine touch the whole heap before the game
//! starts — and that costs more than a second on a machine that has to
//! fault in two gigabytes of pages.
//!
//! Nothing here is a matter of taste. A flag enters the list of the
//! measured profile only when a measurement on this machine showed that
//! it wins, and the numbers of that measurement travel with the flag,
//! so that the interface can say *why* a flag is on and not merely that
//! it is.
//!
//! The launcher never deletes an argument of the version: the arguments
//! of the player come after the arguments of the version, and the last
//! of a pair `+Flag` / `-Flag` is the one the virtual machine obeys.
//! So switching a flag off means appending `-XX:-Flag`.

use serde::{Deserialize, Serialize};

/// The profile of the flags of the virtual machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FlagsProfile {
    /// Exactly what the version of the game prescribes.
    #[default]
    Default,
    /// What the version prescribes, corrected by the measurement that
    /// was made on this machine.
    Measured,
}

impl FlagsProfile {
    /// The name of the profile, as it is written in the settings.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Measured => "measured",
        }
    }

    /// Read the profile from the name it is written under.
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        match name {
            "measured" => Self::Measured,
            _ => Self::Default,
        }
    }
}

/// What the measurement said about one flag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlagDecision {
    /// The flag itself.
    pub flag: String,
    /// Whether the flag is in effect with this profile.
    pub applied: bool,
    /// What the measurement showed, in milliseconds.
    ///
    /// A positive number means the flag costs that much; a negative one
    /// means it saves that much.
    pub effect_ms: i64,
    /// Why the decision was made, in the language of the interface.
    pub reason: String,
}

/// The flag that the modern versions prescribe and that the measurement
/// showed to be the most expensive one.
pub const ALWAYS_PRETOUCH: &str = "-XX:+AlwaysPreTouch";

/// The measurement of the flag `-XX:+AlwaysPreTouch`.
///
/// The numbers are the medians of thirty runs of the virtual machine of
/// Mojang on this machine, with the flags of the version 26.3 and
/// nothing else: the virtual machine was started with the flags of the
/// version, it built its heap — which is where the flag acts — found no
/// class to run and left. The first line is the way the version
/// prescribes it, the second is the same thing with the flag switched
/// off.
///
/// The flag is not measured with `-version`: that answer is given before
/// the heap is built, so a machine that is asked for its version never
/// touches the pages the flag is about.
///
/// The runs are in `benchmarks/jvm-flags.json`, and the second
/// measurement of the same thing is in `benchmarks/jvm-flags-recheck.json`:
/// it says the same, and the number below is the one of the first
/// measurement.
///
/// **The noise of these series is above the threshold of the protocol**
/// (`docs/PROTOCOL.md`, §5), so the protocol gives no verdict for them
/// either. What it does give is the fact that the two ranges do not
/// meet: not one run without the flag is as slow as the fastest run
/// with it. The decision rests on that, and on the size of the
/// difference — it is a large part of the start itself, and the scatter
/// of the series is a part of it several times smaller. The profile is
/// off by default: the player turns it on.
pub const ALWAYS_PRETOUCH_MEASURED_MS: i64 = 973;

/// The same measurement with the flag switched off.
pub const ALWAYS_PRETOUCH_OFF_MEASURED_MS: i64 = 246;

/// The decisions of the profile, in the order they are shown.
#[must_use]
pub fn decisions(profile: FlagsProfile) -> Vec<FlagDecision> {
    match profile {
        FlagsProfile::Default => vec![FlagDecision {
            flag: ALWAYS_PRETOUCH.to_string(),
            applied: true,
            effect_ms: ALWAYS_PRETOUCH_MEASURED_MS - ALWAYS_PRETOUCH_OFF_MEASURED_MS,
            reason: "Так предписывает версия игры: виртуальная машина трогает всю кучу заранее"
                .into(),
        }],
        FlagsProfile::Measured => vec![FlagDecision {
            flag: ALWAYS_PRETOUCH.to_string(),
            applied: false,
            effect_ms: ALWAYS_PRETOUCH_OFF_MEASURED_MS - ALWAYS_PRETOUCH_MEASURED_MS,
            reason: "Замер на этой машине: без этого флага виртуальная машина стартует быстрее"
                .into(),
        }],
    }
}

/// The decisions of every profile.
///
/// The interface shows the list of the profile the player is choosing,
/// so it needs the list of all of them and not only the one that is
/// saved at the moment.
#[must_use]
pub fn every_profile() -> Vec<(FlagsProfile, Vec<FlagDecision>)> {
    vec![
        (FlagsProfile::Default, decisions(FlagsProfile::Default)),
        (FlagsProfile::Measured, decisions(FlagsProfile::Measured)),
    ]
}

/// What the measurement said about one collector of the garbage.
///
/// The player may pick a collector instead of the one the version of the
/// game prescribes, and that is a choice about a flag of the virtual
/// machine. The rule of the project is the same for it as for every
/// other flag: the choice is only offered with the measurement next to
/// it, so that the player knows what the choice costs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectorMeasurement {
    /// The name of the collector, as it is written in the settings.
    pub name: String,
    /// The flag that turns the collector on.
    pub flag: String,
    /// The start of the virtual machine with this collector, in
    /// milliseconds.
    pub start_ms: i64,
    /// The resident memory of the game with this collector, in
    /// mebibytes.
    ///
    /// Zero when the collector was not measured with the game itself:
    /// the measurement of the game is long, and not every collector was
    /// worth that hour.
    pub game_rss_mib: i64,
    /// What the measurement showed, in the language of the interface.
    pub reason: String,
}

/// The start of the virtual machine with ZGC — the collector the version
/// of the game prescribes.
pub const COLLECTOR_Z_START_MS: i64 = 246;
/// The start of the virtual machine with G1.
pub const COLLECTOR_G1_START_MS: i64 = 311;
/// The start of the virtual machine with Parallel.
pub const COLLECTOR_PARALLEL_START_MS: i64 = 280;
/// The resident memory of the game with ZGC, in mebibytes.
pub const COLLECTOR_Z_GAME_RSS_MIB: i64 = 1758;
/// The resident memory of the game with G1, in mebibytes.
pub const COLLECTOR_G1_GAME_RSS_MIB: i64 = 1499;

/// The measurements of the collectors, in the order the settings show
/// them.
///
/// The start of the virtual machine is the middle of thirty runs of the
/// virtual machine of Mojang with the flags of the version 26.3, with
/// the collector of the variant and **without** `-XX:+AlwaysPreTouch` —
/// the pretouch is switched off in every one of them, so that the only
/// thing that differs between the numbers is the collector. It is taken
/// from `benchmarks/jvm-flags.json`.
///
/// The memory of the game is the middle of the runs of the game itself
/// with the profile of the collector — `measured` for ZGC and
/// `measured_g1` for G1 — and it is taken from
/// `benchmarks/flags-ab.json`. **The pair of these two is noisier than
/// five per cent**, so the protocol compares no two of them: these
/// numbers are a measurement and not a verdict. The comparison the
/// protocol does allow is `default` against `measured_g1`, and it is in
/// `docs/RESULTS.md`.
#[must_use]
pub fn collectors() -> Vec<CollectorMeasurement> {
    vec![
        CollectorMeasurement {
            name: "G1".to_string(),
            flag: "-XX:+UseG1GC".to_string(),
            start_ms: COLLECTOR_G1_START_MS,
            game_rss_mib: COLLECTOR_G1_GAME_RSS_MIB,
            reason: "Виртуальная машина стартует дольше, а игра занимает меньше памяти".into(),
        },
        CollectorMeasurement {
            name: "Z".to_string(),
            flag: "-XX:+UseZGC".to_string(),
            start_ms: COLLECTOR_Z_START_MS,
            game_rss_mib: COLLECTOR_Z_GAME_RSS_MIB,
            reason: "Сборщик, который предписывает версия игры: старт и память измерены".into(),
        },
        CollectorMeasurement {
            name: "Parallel".to_string(),
            flag: "-XX:+UseParallelGC".to_string(),
            start_ms: COLLECTOR_PARALLEL_START_MS,
            game_rss_mib: 0,
            reason: "Замер только на старте виртуальной машины".into(),
        },
    ]
}

/// The arguments the profile adds to the ones of the version.
///
/// The arguments of the version are not touched: the arguments of the
/// profile come after them, and the last of a pair `+Flag` / `-Flag` is
/// the one the virtual machine obeys.
#[must_use]
pub fn adjustment(profile: FlagsProfile) -> Vec<String> {
    match profile {
        FlagsProfile::Default => Vec::new(),
        FlagsProfile::Measured => vec!["-XX:-AlwaysPreTouch".to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_profile_is_read_from_its_name() {
        assert_eq!(FlagsProfile::from_name("measured"), FlagsProfile::Measured);
        assert_eq!(FlagsProfile::from_name("default"), FlagsProfile::Default);
        // A name that is not known is the profile of the version: the
        // launcher does not argue with the version because of a typo.
        assert_eq!(FlagsProfile::from_name("nonsense"), FlagsProfile::Default);
    }

    #[test]
    fn the_profile_survives_the_settings() {
        let profile = FlagsProfile::Measured;
        let text = serde_json::to_string(&profile).unwrap();
        assert_eq!(text, "\"measured\"");
        assert_eq!(
            serde_json::from_str::<FlagsProfile>(&text).unwrap(),
            FlagsProfile::Measured
        );
    }

    #[test]
    fn the_default_profile_does_not_add_anything() {
        assert_eq!(adjustment(FlagsProfile::Default), Vec::<String>::new());
    }

    #[test]
    fn the_measured_profile_switches_the_flag_off() {
        assert_eq!(
            adjustment(FlagsProfile::Measured),
            vec!["-XX:-AlwaysPreTouch".to_string()]
        );
    }

    #[test]
    fn the_decision_says_which_way_the_flag_goes() {
        let default = decisions(FlagsProfile::Default);
        assert_eq!(default.len(), 1);
        assert!(default[0].applied);
        assert!(default[0].effect_ms > 0, "the flag costs time");

        let measured = decisions(FlagsProfile::Measured);
        assert!(!measured[0].applied);
        assert!(measured[0].effect_ms < 0, "the flag is switched off");
        assert_eq!(measured[0].effect_ms, -default[0].effect_ms);
    }

    #[test]
    fn every_decision_has_a_reason() {
        for profile in [FlagsProfile::Default, FlagsProfile::Measured] {
            for decision in decisions(profile) {
                assert!(
                    !decision.reason.is_empty(),
                    "a decision without a reason is a decision nobody can check"
                );
                assert_ne!(decision.flag, "");
            }
        }
    }

    #[test]
    fn every_collector_has_a_measurement_and_a_reason() {
        // A collector is a flag of the virtual machine, and the rule of
        // the project is the same for it as for every other flag: the
        // choice is offered with the measurement next to it.
        let measured = collectors();

        assert!(!measured.is_empty(), "there is nothing to show");
        for collector in &measured {
            assert!(
                !collector.reason.is_empty(),
                "a collector without a reason is a choice nobody can check"
            );
            assert!(
                collector.flag.starts_with("-XX:+"),
                "{} is not a flag that turns a collector on",
                collector.flag
            );
            assert!(
                collector.start_ms > 0,
                "{} has no measured start of the virtual machine",
                collector.name
            );
        }
    }

    #[test]
    fn the_collectors_of_the_settings_are_all_measured() {
        // The settings offer the collectors by name, and every name
        // they offer has to be one the measurement knows: a choice
        // without a measurement is a claim, and claims are not shipped.
        let collectors = collectors();
        let measured: Vec<&str> = collectors.iter().map(|c| c.name.as_str()).collect();
        for name in ["G1", "Z", "Parallel"] {
            assert!(
                measured.contains(&name),
                "{name} is offered in the settings but was not measured"
            );
        }
    }
}
