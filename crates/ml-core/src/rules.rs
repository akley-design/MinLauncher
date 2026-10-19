//! Mojang-style rule evaluation.
//!
//! Version JSONs describe which libraries and JVM arguments apply to a given
//! operating system / architecture via a small rule language. The same
//! evaluator is used to pick libraries *and* to filter JVM arguments, so the
//! two can never disagree about what "this platform" means.

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
    /// 64-bit x86_64.
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
}

impl Platform {
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
        }
    }
}

/// The `os` or `arch` half of a rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsCondition {
    /// Name of the OS / architecture to match.
    #[serde(rename = "name")]
    pub name: Option<String>,
    /// Alternative OS / architecture to match.
    #[serde(rename = "version")]
    pub version: Option<String>,
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
                Some(Self {
                    lower: VersionBound::parse(rest),
                    upper: None,
                    lower_inclusive: raw.starts_with(">="),
                    upper_inclusive: true,
                })
            }
            b'<' => {
                let rest = raw.strip_prefix("<=").or_else(|| raw.strip_prefix('<'))?;
                Some(Self {
                    lower: None,
                    upper: VersionBound::parse(rest),
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
#[must_use]
pub fn evaluate(rules: &[Rule], platform: Platform, version: Option<&str>) -> RuleOutcome {
    let mut outcome = RuleOutcome::Unmatched;

    for rule in rules {
        if !rule_matches(rule, platform, version) {
            continue;
        }
        outcome = match rule.action {
            RuleAction::Allow => RuleOutcome::Allowed,
            RuleAction::Disallow => RuleOutcome::Disallowed,
        };
    }

    outcome
}

fn rule_matches(rule: &Rule, platform: Platform, version: Option<&str>) -> bool {
    if let Some(os) = &rule.os {
        if !condition_matches(os, platform.os.as_str(), platform.arch.as_str()) {
            return false;
        }
    }
    if let Some(arch) = &rule.arch {
        if !condition_matches(arch, platform.arch.as_str(), platform.os.as_str()) {
            return false;
        }
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
    true
}

fn condition_matches(condition: &OsCondition, primary: &str, fallback: &str) -> bool {
    if let Some(name) = &condition.name {
        if name == primary || name == fallback {
            return true;
        }
    }
    if let Some(version) = &condition.version {
        if version == primary || version == fallback {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn platform() -> Platform {
        Platform {
            os: OsName::Windows,
            arch: OsArch::X86_64,
            jvm_bits: 64,
        }
    }

    #[test]
    fn plain_range_parses() {
        let r = VersionRange::parse(">=1.17").unwrap();
        assert!(r.matches("1.20.1"));
        assert!(!r.matches("1.16.5"));
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
            Rule {
                action: RuleAction::Allow,
                os: None,
                arch: None,
                version: None,
            },
            Rule {
                action: RuleAction::Disallow,
                os: Some(OsCondition {
                    name: Some("windows".into()),
                    version: None,
                }),
                arch: None,
                version: None,
            },
        ];
        assert_eq!(evaluate(&rules, platform(), None), RuleOutcome::Disallowed);
    }

    #[test]
    fn os_rule_matches_windows() {
        let rules = vec![Rule {
            action: RuleAction::Allow,
            os: Some(OsCondition {
                name: Some("windows".into()),
                version: None,
            }),
            arch: None,
            version: None,
        }];
        assert_eq!(evaluate(&rules, platform(), None), RuleOutcome::Allowed);
    }

    #[test]
    fn arch_rule_matches_current_arch() {
        let rules = vec![Rule {
            action: RuleAction::Allow,
            os: None,
            arch: Some(OsCondition {
                name: Some("x86_64".into()),
                version: None,
            }),
            version: None,
        }];
        assert_eq!(evaluate(&rules, platform(), None), RuleOutcome::Allowed);
    }

    #[test]
    fn unmatched_is_not_allowed() {
        assert_eq!(evaluate(&[], platform(), None), RuleOutcome::Unmatched);
        assert!(!RuleOutcome::Unmatched.is_allowed());
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
}
