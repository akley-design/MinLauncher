//! Mojang-style rule evaluation.
//!
//! Version JSONs describe which libraries and JVM arguments apply to a given
//! operating system / architecture via a small rule language. The same
//! evaluator is used to pick libraries *and* to filter JVM arguments, so the
//! two can never disagree about what "this platform" means.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// The operating systems Mojang distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OsName {
    /// Windows.
    Windows,
    /// macOS.
    Osx,
    /// Linux.
    Linux,
    /// Unknown / unsupported.
    Unknown,
}

impl OsName {
    /// The canonical `os.name` value used by Mojang's rule matching.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Osx => "osx",
            Self::Linux => "linux",
            Self::Unknown => "unknown",
        }
    }
}

/// CPU architectures Mojang distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OsArch {
    /// 32-bit x86.
    X86,
    /// 64-bit `x86_64`.
    X86_64,
    /// 64-bit ARM.
    Arm64,
    /// Unknown / unsupported.
    Unknown,
}

impl OsArch {
    /// The canonical `os.arch` value used by Mojang's rule matching.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::X86 => "x86",
            Self::X86_64 => "x86_64",
            Self::Arm64 => "arm64",
            Self::Unknown => "unknown",
        }
    }
}

/// The concrete platform the launcher is running on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Platform {
    /// Operating system.
    pub os: OsName,
    /// CPU architecture.
    pub arch: OsArch,
    /// JVM bitness that Mojang uses in the `arch` rule (`x86` / `x86_64`).
    pub jvm_bits: u32,
    /// The operating system's version, e.g. `10.0.22621` on Windows.
    ///
    /// Mojang matches this against the `os.version` regex and the
    /// `os.versionRange` bounds, so it has to be the *full* version and
    /// not just the build number.
    pub os_version: Option<&'static str>,
}

impl Platform {
    /// A platform with the given operating system and architecture.
    ///
    /// The OS version is left unset, which means rules that constrain the
    /// version cannot fire.
    #[must_use]
    pub fn new(os: OsName, arch: OsArch) -> Self {
        Self {
            os,
            arch,
            jvm_bits: if arch == OsArch::X86 { 32 } else { 64 },
            os_version: None,
        }
    }

    /// Set the operating system version.
    #[must_use]
    pub fn with_os_version(mut self, version: Option<&'static str>) -> Self {
        self.os_version = version;
        self
    }

    /// Detect the host platform.
    #[must_use]
    pub fn current() -> Self {
        let os = if cfg!(target_os = "windows") {
            OsName::Windows
        } else if cfg!(target_os = "macos") {
            OsName::Osx
        } else if cfg!(target_os = "linux") {
            OsName::Linux
        } else {
            OsName::Unknown
        };

        let arch = if cfg!(target_arch = "x86") {
            OsArch::X86
        } else if cfg!(target_arch = "x86_64") {
            OsArch::X86_64
        } else if cfg!(target_arch = "aarch64") {
            OsArch::Arm64
        } else {
            OsArch::Unknown
        };

        Self {
            os,
            arch,
            jvm_bits: if cfg!(target_pointer_width = "64") {
                64
            } else {
                32
            },
            os_version: crate::sys::os_version(),
        }
    }
}

/// A range of operating system versions, e.g. `10.0.17134`.
///
/// Mojang uses this to distinguish Windows builds: the launcher has to know
/// whether the host is newer or older than a given build, because the
/// garbage collector flags differ between them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsVersionRange {
    /// The lowest version that matches, inclusive.
    #[serde(default)]
    pub min: Option<String>,
    /// The highest version that matches, inclusive.
    #[serde(default)]
    pub max: Option<String>,
}

/// The `os` or `arch` half of a rule.
///
/// Mojang uses three different spellings inside the `os` block:
///
/// * `name` — the operating system (`windows`, `osx`, `linux`),
/// * `version` — a *regular expression* matched against the OS version
///   (Windows only, e.g. `^10\.` for Windows 10 and newer),
/// * `arch` — the CPU architecture, occasionally combined with the OS,
/// * `versionRange` — inclusive `min`/`max` bounds on the OS version.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsCondition {
    /// Name of the OS / architecture to match.
    #[serde(rename = "name")]
    pub name: Option<String>,
    /// A regular expression matched against the OS version.
    #[serde(rename = "version")]
    pub version: Option<String>,
    /// The architecture, when the condition names one.
    #[serde(rename = "arch")]
    pub arch: Option<String>,
    /// Inclusive bounds on the OS version.
    #[serde(default, rename = "versionRange")]
    pub version_range: Option<OsVersionRange>,
}

impl OsCondition {
    /// A condition that names an operating system.
    #[must_use]
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            ..Self::default()
        }
    }
}

/// A rule: an action plus the conditions under which it applies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// `allow` enables the guarded value, `disallow` forbids it.
    pub action: RuleAction,
    /// Conditions that must hold for the rule to fire.
    #[serde(default, rename = "os")]
    pub os: Option<OsCondition>,
    /// Architecture conditions.
    #[serde(default, rename = "arch")]
    pub arch: Option<OsCondition>,
    /// Version conditions.
    #[serde(default)]
    pub version: Option<String>,
    /// Feature conditions.
    ///
    /// A rule with a feature condition only fires when *every* named
    /// feature is enabled. This is what keeps `--demo` and the quick-play
    /// flags out of a normal launch.
    #[serde(default)]
    pub features: Option<BTreeMap<String, bool>>,
}

/// What a [`Rule`] does when it matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    /// Include the guarded value.
    Allow,
    /// Exclude the guarded value.
    Disallow,
}

/// The version range format used inside `version` rule strings.
///
/// Mojang encodes these as `">=1.7"`, `"<1.8"`, `"[1.8,1.9)"` and so on.
/// Only the subset that actually appears in shipped version manifests is
/// supported, which keeps the implementation small and dependency-free.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRange {
    lower: Option<VersionBound>,
    upper: Option<VersionBound>,
    lower_inclusive: bool,
    upper_inclusive: bool,
}

/// One endpoint of a [`VersionRange`].
#[derive(Debug, Clone, PartialEq, Eq)]
struct VersionBound {
    segments: [u64; 3],
    raw: String,
}

impl VersionRange {
    /// Parse a Mojang version range expression.
    ///
    /// # Errors
    /// Returns `None` for expressions the launcher does not understand, in
    /// which case the caller must decide what to do (the launcher treats an
    /// unparseable range as "does not match").
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        let first = raw.as_bytes().first().copied()?;

        match first {
            // Bounded range: "[1.8,1.9)" — the opening and closing brackets
            // may differ, and each one decides inclusivity of its own bound.
            b'[' | b'(' => {
                let body = &raw[1..];
                let body = body
                    .strip_suffix(']')
                    .or_else(|| body.strip_suffix(')'))
                    .unwrap_or(body);
                let (lo, hi) = body.split_once(',')?;
                Some(Self {
                    lower: VersionBound::parse(lo),
                    upper: VersionBound::parse(hi),
                    lower_inclusive: first == b'[',
                    upper_inclusive: raw.ends_with(']'),
                })
            }
            b'>' => {
                let rest = raw.strip_prefix(">=").or_else(|| raw.strip_prefix('>'))?;
                // A range with no bound after the sign matches every
                // version — a rule the document never wrote. An empty
                // bound is a range that cannot be parsed, and an
                // unparsable range must not match anything.
                let lower = VersionBound::parse(rest)?;
                Some(Self {
                    lower: Some(lower),
                    upper: None,
                    lower_inclusive: raw.starts_with(">="),
                    upper_inclusive: true,
                })
            }
            b'<' => {
                let rest = raw.strip_prefix("<=").or_else(|| raw.strip_prefix('<'))?;
                let upper = VersionBound::parse(rest)?;
                Some(Self {
                    lower: None,
                    upper: Some(upper),
                    lower_inclusive: true,
                    upper_inclusive: raw.starts_with("<="),
                })
            }
            _ => None,
        }
    }

    /// Test a version string against this range.
    #[must_use]
    pub fn matches(&self, version: &str) -> bool {
        let segments = VersionBound::segments_of(version);
        if let Some(lower) = &self.lower {
            match segments.cmp(&lower.segments) {
                std::cmp::Ordering::Less => return false,
                std::cmp::Ordering::Equal if !self.lower_inclusive => return false,
                _ => {}
            }
        }
        if let Some(upper) = &self.upper {
            match segments.cmp(&upper.segments) {
                std::cmp::Ordering::Greater => return false,
                std::cmp::Ordering::Equal if !self.upper_inclusive => return false,
                _ => {}
            }
        }
        true
    }
}

impl VersionBound {
    fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }
        Some(Self {
            segments: Self::segments_of(raw),
            raw: raw.to_string(),
        })
    }

    /// Split a version into exactly three numeric segments.
    ///
    /// Missing segments count as zero, so `1.8` and `1.8.0` are equal while
    /// `1.8.9` is strictly greater than `1.8`. This is what makes a range
    /// such as `[1.8,1.9)` accept `1.8.9`.
    fn segments_of(raw: &str) -> [u64; 3] {
        let mut out = [0u64; 3];
        for (index, part) in raw.split(['.', '-', '_']).take(3).enumerate() {
            out[index] = part.parse::<u64>().unwrap_or(0);
        }
        out
    }
}

impl std::fmt::Display for VersionBound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.raw)
    }
}

/// Result of evaluating a list of rules against a platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleOutcome {
    /// The value should be included.
    Allowed,
    /// The value should be excluded.
    Disallowed,
    /// No rule matched, so the value's default state applies.
    Unmatched,
}

impl RuleOutcome {
    /// Whether the guarded value ends up enabled.
    #[must_use]
    pub fn is_allowed(self) -> bool {
        matches!(self, Self::Allowed)
    }
}

/// Evaluate a list of rules the way Mojang's launcher does.
///
/// The last matching rule wins, which is what the vanilla launcher does and
/// what every third-party launcher has to replicate for the game to boot.
///
/// An *empty* rule list means "no conditions", so the value applies. That is
/// how a library without a `rules` block ends up in every installation, and
/// how the `default-user-jvm` block carries memory settings that are not
/// tied to a platform.
#[must_use]
pub fn evaluate(rules: &[Rule], platform: Platform, version: Option<&str>) -> RuleOutcome {
    evaluate_with_features(rules, platform, version, &FeatureSet::default())
}

/// Evaluate a list of rules, taking the enabled features into account.
///
/// # Panics
/// Never.
#[must_use]
pub fn evaluate_with_features(
    rules: &[Rule],
    platform: Platform,
    version: Option<&str>,
    features: &FeatureSet,
) -> RuleOutcome {
    if rules.is_empty() {
        return RuleOutcome::Allowed;
    }

    let mut outcome = RuleOutcome::Unmatched;

    for rule in rules {
        if !rule_matches(rule, platform, version, features) {
            continue;
        }
        outcome = match rule.action {
            RuleAction::Allow => RuleOutcome::Allowed,
            RuleAction::Disallow => RuleOutcome::Disallowed,
        };
    }

    outcome
}

/// The set of launcher features that are enabled for a launch.
///
/// Mojang gates a few game arguments behind features — `--demo` for a demo
/// account, the quick-play flags for a quick-play session. The launcher
/// decides which of them are on; the rule engine then keeps the matching
/// arguments out of the command line.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureSet {
    /// The enabled features, by name.
    pub enabled: BTreeSet<String>,
}

impl FeatureSet {
    /// An empty feature set: nothing is enabled.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// A feature set with the given features enabled.
    #[must_use]
    pub fn of<I, S>(features: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            enabled: features.into_iter().map(Into::into).collect(),
        }
    }

    /// Enable a feature.
    pub fn enable(&mut self, feature: impl Into<String>) {
        self.enabled.insert(feature.into());
    }

    /// Whether a feature is enabled.
    #[must_use]
    pub fn is_enabled(&self, feature: &str) -> bool {
        self.enabled.contains(feature)
    }

    /// Whether the set satisfies a rule's feature condition.
    ///
    /// Every feature the condition names has to be enabled, and a feature
    /// the condition names as `false` has to be disabled.
    #[must_use]
    pub fn satisfies(&self, condition: &BTreeMap<String, bool>) -> bool {
        condition
            .iter()
            .all(|(name, expected)| self.is_enabled(name) == *expected)
    }
}

fn rule_matches(
    rule: &Rule,
    platform: Platform,
    version: Option<&str>,
    features: &FeatureSet,
) -> bool {
    if let Some(os) = &rule.os
        && !os_condition_matches(os, platform)
    {
        return false;
    }
    if let Some(arch) = &rule.arch
        && !arch_condition_matches(arch, platform)
    {
        return false;
    }
    if let Some(range) = &rule.version {
        let Some(version) = version else { return false };
        let Some(parsed) = VersionRange::parse(range) else {
            return false;
        };
        if !parsed.matches(version) {
            return false;
        }
    }
    if let Some(condition) = &rule.features
        && !features.satisfies(condition)
    {
        return false;
    }
    true
}

/// Whether an `os` condition holds on this platform.
///
/// The condition can name the operating system, constrain the version with a
/// regular expression, constrain the version with inclusive bounds, and name
/// an architecture. All of the constraints that are present have to hold.
fn os_condition_matches(condition: &OsCondition, platform: Platform) -> bool {
    if let Some(name) = &condition.name
        && name != platform.os.as_str()
    {
        return false;
    }

    if let Some(arch) = &condition.arch
        && arch != platform.arch.as_str()
    {
        return false;
    }

    if condition.version.is_some() || condition.version_range.is_some() {
        let Some(os_version) = platform.os_version else {
            // The host version is unknown, so a version-constrained rule
            // cannot be satisfied. Mojang's own launcher behaves the same
            // way when it cannot determine the version.
            return false;
        };

        if let Some(pattern) = &condition.version
            && !version_regex_matches(pattern, os_version)
        {
            return false;
        }

        if let Some(range) = &condition.version_range
            && !version_range_matches(range, os_version)
        {
            return false;
        }
    }

    true
}

/// Whether an `arch` condition holds on this platform.
fn arch_condition_matches(condition: &OsCondition, platform: Platform) -> bool {
    if let Some(name) = &condition.name
        && name != platform.arch.as_str()
    {
        return false;
    }
    if let Some(arch) = &condition.arch
        && arch != platform.arch.as_str()
    {
        return false;
    }
    true
}

/// Match an OS version against Mojang's regular expression syntax.
///
/// Mojang writes these as `^10\.` and `^10\.5\.\d$`. The `regex` crate
/// understands that syntax directly, but a pattern that does not compile
/// must not take the launcher down — it simply does not match.
fn version_regex_matches(pattern: &str, version: &str) -> bool {
    match regex::Regex::new(pattern) {
        Ok(regex) => regex.is_match(version),
        Err(error) => {
            tracing::warn!(pattern, %error, "ignoring an unparsable os.version pattern");
            false
        }
    }
}

/// Match an OS version against inclusive `min` / `max` bounds.
///
/// The versions are compared segment by segment as numbers, so `10.0.17134`
/// is *older* than `10.0.22621` and *newer* than `10.0.19045`.
fn version_range_matches(range: &OsVersionRange, version: &str) -> bool {
    let actual = VersionBound::segments_of(version);

    if let Some(min) = &range.min {
        let min = VersionBound::segments_of(min);
        if actual < min {
            return false;
        }
    }
    if let Some(max) = &range.max {
        let max = VersionBound::segments_of(max);
        if actual > max {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn platform() -> Platform {
        Platform {
            os: OsName::Windows,
            arch: OsArch::X86_64,
            jvm_bits: 64,
            os_version: Some("10.0.26300"),
        }
    }

    fn rule(action: RuleAction, os: Option<OsCondition>) -> Rule {
        Rule {
            action,
            os,
            arch: None,
            version: None,
            features: None,
        }
    }

    #[test]
    fn plain_range_parses() {
        let r = VersionRange::parse(">=1.17").unwrap();
        assert!(r.matches("1.20.1"));
        assert!(!r.matches("1.16.5"));
    }

    #[test]
    fn a_sign_with_no_bound_matches_nothing() {
        // `>` without a version after it is not a rule that matches every
        // version: it is a range that cannot be parsed, and an unparsable
        // range must not match anything.
        for raw in [">", ">=", "<", "<="] {
            assert!(VersionRange::parse(raw).is_none(), "{raw}");
        }
    }

    #[test]
    fn exclusive_upper_bound() {
        let r = VersionRange::parse("[1.8,1.9)").unwrap();
        assert!(r.matches("1.8.9"));
        assert!(!r.matches("1.9"));
        assert!(!r.matches("1.9.1"));
    }

    #[test]
    fn disallow_wins_when_it_is_last() {
        let rules = vec![
            rule(RuleAction::Allow, None),
            rule(RuleAction::Disallow, Some(OsCondition::named("windows"))),
        ];
        assert_eq!(evaluate(&rules, platform(), None), RuleOutcome::Disallowed);
    }

    #[test]
    fn os_rule_matches_windows() {
        let rules = vec![rule(RuleAction::Allow, Some(OsCondition::named("windows")))];
        assert_eq!(evaluate(&rules, platform(), None), RuleOutcome::Allowed);
    }

    #[test]
    fn arch_rule_matches_current_arch() {
        let rules = vec![Rule {
            action: RuleAction::Allow,
            os: None,
            arch: Some(OsCondition::named("x86_64")),
            version: None,
            features: None,
        }];
        assert_eq!(evaluate(&rules, platform(), None), RuleOutcome::Allowed);
    }

    #[test]
    fn unmatched_is_not_allowed() {
        let rules = vec![rule(RuleAction::Allow, Some(OsCondition::named("linux")))];
        assert_eq!(evaluate(&rules, platform(), None), RuleOutcome::Unmatched);
        assert!(!RuleOutcome::Unmatched.is_allowed());
    }

    #[test]
    fn an_empty_rule_list_allows_the_value() {
        // A library without a `rules` block belongs to every installation.
        assert_eq!(evaluate(&[], platform(), None), RuleOutcome::Allowed);
    }

    #[test]
    fn an_unconditional_rule_matches() {
        // `{"action": "allow"}` — the idiom Mojang uses to allow by
        // default and then disallow on a specific platform.
        let rules = vec![rule(RuleAction::Allow, None)];
        assert_eq!(evaluate(&rules, platform(), None), RuleOutcome::Allowed);
    }

    #[test]
    fn year_based_versions_compare_numerically() {
        // Minecraft moved to a year-based scheme (26.x) in 2026. A range
        // written in the 1.x scheme must still compare numerically instead
        // of failing to parse, so that rules never silently stop matching.
        let r = VersionRange::parse(">=1.17").unwrap();
        assert!(r.matches("1.21.11"));
        assert!(r.matches("26.3"));
        assert!(!r.matches("1.16.5"));
    }

    #[test]
    fn os_version_regex_matches_windows_ten_and_newer() {
        let rules = vec![rule(
            RuleAction::Allow,
            Some(OsCondition {
                name: Some("windows".into()),
                version: Some(r"^10\.".into()),
                ..OsCondition::default()
            }),
        )];
        assert_eq!(evaluate(&rules, platform(), None), RuleOutcome::Allowed);
    }

    #[test]
    fn os_version_regex_that_does_not_match_is_skipped() {
        let rules = vec![rule(
            RuleAction::Allow,
            Some(OsCondition {
                name: Some("osx".into()),
                version: Some(r"^10\.5\.\d$".into()),
                ..OsCondition::default()
            }),
        )];
        assert_eq!(evaluate(&rules, platform(), None), RuleOutcome::Unmatched);
    }

    #[test]
    fn a_version_constraint_needs_a_known_os_version() {
        let rules = vec![rule(
            RuleAction::Allow,
            Some(OsCondition {
                name: Some("windows".into()),
                version_range: Some(OsVersionRange {
                    min: Some("10.0.17134".into()),
                    max: None,
                }),
                ..OsCondition::default()
            }),
        )];

        let unknown = Platform {
            os_version: None,
            ..platform()
        };
        assert_eq!(evaluate(&rules, unknown, None), RuleOutcome::Unmatched);
    }

    #[test]
    fn a_minimum_version_range_excludes_older_builds() {
        let rules = vec![rule(
            RuleAction::Allow,
            Some(OsCondition {
                name: Some("windows".into()),
                version_range: Some(OsVersionRange {
                    min: Some("10.0.17134".into()),
                    max: None,
                }),
                ..OsCondition::default()
            }),
        )];

        // Windows 11 is newer than the bound.
        assert_eq!(evaluate(&rules, platform(), None), RuleOutcome::Allowed);

        // Windows 10 1803 is exactly the bound and therefore matches.
        let at_bound = Platform {
            os_version: Some("10.0.17134"),
            ..platform()
        };
        assert_eq!(evaluate(&rules, at_bound, None), RuleOutcome::Allowed);

        // Windows 10 1709 is older and does not.
        let below = Platform {
            os_version: Some("10.0.16299"),
            ..platform()
        };
        assert_eq!(evaluate(&rules, below, None), RuleOutcome::Unmatched);
    }

    #[test]
    fn a_maximum_version_range_excludes_newer_builds() {
        let rules = vec![rule(
            RuleAction::Allow,
            Some(OsCondition {
                name: Some("windows".into()),
                version_range: Some(OsVersionRange {
                    min: None,
                    max: Some("10.0.17134".into()),
                }),
                ..OsCondition::default()
            }),
        )];

        // The legacy G1GC flags must not be applied on Windows 11.
        assert_eq!(evaluate(&rules, platform(), None), RuleOutcome::Unmatched);

        let old = Platform {
            os_version: Some("10.0.16299"),
            ..platform()
        };
        assert_eq!(evaluate(&rules, old, None), RuleOutcome::Allowed);
    }

    #[test]
    fn the_two_garbage_collector_rules_are_mutually_exclusive() {
        // This is the exact pair Mojang ships in `default-user-jvm`: the
        // ZGC flags for modern Windows, the legacy G1GC flags for the
        // builds before it. Applying both at once would break the game.
        let zgc = rule(
            RuleAction::Allow,
            Some(OsCondition {
                name: Some("windows".into()),
                version_range: Some(OsVersionRange {
                    min: Some("10.0.17134".into()),
                    max: None,
                }),
                ..OsCondition::default()
            }),
        );
        let g1gc = rule(
            RuleAction::Allow,
            Some(OsCondition {
                name: Some("windows".into()),
                version_range: Some(OsVersionRange {
                    min: None,
                    max: Some("10.0.17134".into()),
                }),
                ..OsCondition::default()
            }),
        );

        let modern = platform();
        assert!(evaluate(std::slice::from_ref(&zgc), modern, None).is_allowed());
        assert!(!evaluate(std::slice::from_ref(&g1gc), modern, None).is_allowed());

        let legacy = Platform {
            os_version: Some("10.0.16299"),
            ..platform()
        };
        assert!(!evaluate(&[zgc], legacy, None).is_allowed());
        assert!(evaluate(&[g1gc], legacy, None).is_allowed());
    }

    #[test]
    fn a_feature_rule_only_fires_when_the_feature_is_enabled() {
        let rules = vec![Rule {
            action: RuleAction::Allow,
            os: None,
            arch: None,
            version: None,
            features: Some(BTreeMap::from([("is_demo_user".to_string(), true)])),
        }];

        // Without the feature the argument stays out of the command line.
        assert_eq!(
            evaluate_with_features(&rules, platform(), None, &FeatureSet::none()),
            RuleOutcome::Unmatched
        );

        let demo = FeatureSet::of(["is_demo_user"]);
        assert_eq!(
            evaluate_with_features(&rules, platform(), None, &demo),
            RuleOutcome::Allowed
        );
    }

    #[test]
    fn a_feature_rule_can_require_a_feature_to_be_off() {
        let rules = vec![Rule {
            action: RuleAction::Allow,
            os: None,
            arch: None,
            version: None,
            features: Some(BTreeMap::from([("is_demo_user".to_string(), false)])),
        }];

        assert_eq!(
            evaluate_with_features(&rules, platform(), None, &FeatureSet::none()),
            RuleOutcome::Allowed
        );

        let demo = FeatureSet::of(["is_demo_user"]);
        assert_eq!(
            evaluate_with_features(&rules, platform(), None, &demo),
            RuleOutcome::Unmatched
        );
    }

    #[test]
    fn an_os_condition_can_name_the_architecture() {
        let rules = vec![rule(
            RuleAction::Allow,
            Some(OsCondition {
                name: Some("windows".into()),
                arch: Some("x86".into()),
                ..OsCondition::default()
            }),
        )];
        assert_eq!(evaluate(&rules, platform(), None), RuleOutcome::Unmatched);
    }

    #[test]
    fn the_feature_set_reports_what_is_enabled() {
        let mut features = FeatureSet::none();
        assert!(!features.is_enabled("is_demo_user"));
        features.enable("is_demo_user");
        assert!(features.is_enabled("is_demo_user"));
    }
}
