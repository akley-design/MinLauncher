//! The metrics the launcher is measured by.
//!
//! The claim of the project is "at least 20 % faster than the reference
//! launcher", and a claim like that is only meaningful if the point of
//! measurement is fixed beforehand. Every metric therefore names three
//! things:
//!
//! * what exactly is measured,
//! * which direction is an improvement,
//! * the target the result has to reach.
//!
//! The metrics are the ones fixed in `docs/PLAN.md` § 1.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The metrics the launcher is measured by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Metric {
    /// M1 — from the start of the process to the ready window, with an
    /// empty cache.
    LauncherColdStart,
    /// M2 — the same, with a warm cache.
    LauncherWarmStart,
    /// M3 — from the click on "Play" to the window of the game.
    ///
    /// This is the metric the user means when they say "it starts
    /// faster".
    ClickToWindow,
    /// M4 — a full install of a version from scratch.
    FreshInstall,
    /// M5 — the average frame rate in the game.
    InGameFps,
    /// M6 — the 99th percentile of the frame time, i.e. the 1 % low.
    InGameFrametime,
    /// M7 — the memory the launcher process uses.
    LauncherRss,
    /// M8 — the memory the game process uses.
    GameRss,
}

impl Metric {
    /// Every metric, in the order of the plan.
    #[must_use]
    pub fn all() -> &'static [Self] {
        &[
            Self::LauncherColdStart,
            Self::LauncherWarmStart,
            Self::ClickToWindow,
            Self::FreshInstall,
            Self::InGameFps,
            Self::InGameFrametime,
            Self::LauncherRss,
            Self::GameRss,
        ]
    }

    /// The stable name of the metric, as it is stored in the database.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LauncherColdStart => "launcher_cold_start",
            Self::LauncherWarmStart => "launcher_warm_start",
            Self::ClickToWindow => "click_to_window",
            Self::FreshInstall => "fresh_install",
            Self::InGameFps => "in_game_fps",
            Self::InGameFrametime => "in_game_frametime",
            Self::LauncherRss => "launcher_rss",
            Self::GameRss => "game_rss",
        }
    }

    /// The identifier of the metric in the plan.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::LauncherColdStart => "M1",
            Self::LauncherWarmStart => "M2",
            Self::ClickToWindow => "M3",
            Self::FreshInstall => "M4",
            Self::InGameFps => "M5",
            Self::InGameFrametime => "M6",
            Self::LauncherRss => "M7",
            Self::GameRss => "M8",
        }
    }

    /// Which direction is an improvement.
    #[must_use]
    pub fn direction(self) -> Direction {
        match self {
            Self::InGameFps => Direction::Higher,
            _ => Direction::Lower,
        }
    }

    /// The unit of the measurement.
    #[must_use]
    pub fn unit(self) -> &'static str {
        match self {
            Self::InGameFps => "fps",
            Self::LauncherRss | Self::GameRss => "mib",
            _ => "ms",
        }
    }

    /// The target of the metric.
    ///
    /// The value is the **ratio** the measurement of `MinLauncher` may
    /// have to the measurement of the reference launcher: 0.8 means "at
    /// most 80 %", which is the 20 % improvement the project promises.
    /// A metric with an absolute target instead of a relative one
    /// returns `None`.
    #[must_use]
    pub fn target_ratio(self) -> Option<f64> {
        match self {
            Self::LauncherColdStart
            | Self::LauncherWarmStart
            | Self::ClickToWindow
            | Self::FreshInstall => Some(0.8),
            Self::InGameFps => Some(1.1),
            Self::InGameFrametime | Self::GameRss => Some(0.9),
            Self::LauncherRss => None,
        }
    }

    /// The absolute target of the metric, if it has one.
    ///
    /// The memory of the launcher is capped in absolute terms, because
    /// it is not worth comparing against a launcher that was never
    /// measured on the same machine.
    #[must_use]
    pub fn absolute_target(self) -> Option<f64> {
        match self {
            Self::LauncherRss => Some(60.0),
            _ => None,
        }
    }

    /// The suite the metric belongs to.
    #[must_use]
    pub fn suite(self) -> &'static str {
        match self {
            Self::LauncherColdStart | Self::LauncherWarmStart => "launcher",
            Self::ClickToWindow | Self::FreshInstall => "install",
            Self::InGameFps | Self::InGameFrametime => "game",
            Self::LauncherRss | Self::GameRss => "memory",
        }
    }
}

impl fmt::Display for Metric {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which direction of a change is an improvement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    /// A smaller value is better.
    Lower,
    /// A bigger value is better.
    Higher,
}

/// The verdict of a comparison between two measurements.
///
/// The names of the variants are the ones the interface shows, so they
/// are written the way every other name of the interface is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// The target was reached.
    Met,
    /// The target was missed.
    Missed,
    /// There was nothing to compare.
    Unknown,
}

impl Verdict {
    /// Whether the target was reached.
    #[must_use]
    pub fn is_met(self) -> bool {
        self == Self::Met
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_metric_has_a_distinct_name() {
        let mut names: Vec<&str> = Metric::all().iter().map(|metric| metric.as_str()).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "two metrics share a name");
    }

    #[test]
    fn the_codes_of_the_plan_are_kept() {
        assert_eq!(Metric::ClickToWindow.code(), "M3");
        assert_eq!(Metric::InGameFps.code(), "M5");
        assert_eq!(Metric::LauncherRss.code(), "M7");
    }

    #[test]
    fn only_the_frame_rate_wants_to_be_bigger() {
        for metric in Metric::all() {
            if *metric == Metric::InGameFps {
                assert_eq!(metric.direction(), Direction::Higher);
            } else {
                assert_eq!(metric.direction(), Direction::Lower, "{metric}");
            }
        }
    }

    #[test]
    fn the_target_of_the_start_metrics_is_twenty_percent_faster() {
        for metric in [
            Metric::LauncherColdStart,
            Metric::LauncherWarmStart,
            Metric::ClickToWindow,
            Metric::FreshInstall,
        ] {
            assert_eq!(metric.target_ratio(), Some(0.8), "{metric}");
        }
    }

    #[test]
    fn the_memory_of_the_launcher_has_an_absolute_target() {
        assert_eq!(Metric::LauncherRss.absolute_target(), Some(60.0));
        assert_eq!(Metric::LauncherRss.target_ratio(), None);
        assert_eq!(Metric::GameRss.target_ratio(), Some(0.9));
    }

    #[test]
    fn the_units_are_the_expected_ones() {
        assert_eq!(Metric::ClickToWindow.unit(), "ms");
        assert_eq!(Metric::InGameFps.unit(), "fps");
        assert_eq!(Metric::GameRss.unit(), "mib");
    }
}
