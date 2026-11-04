use serde::Serialize;

use crate::privacy::SafeText;
use crate::safe_text;

const MB: u64 = 1_000_000;
const MIB: u64 = 1 << 20;
const MS: u64 = 1_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TargetId {
    ColdUsable,
    ColdUsableSpeedPhase,
    WarmFirstFrame,
    WarmFirstUsable,
    IdlePrivateBytes,
    IdleWakeupsFocused,
    IdleWakeupsMinimised,
    KeyToFrameP95,
    Open100k,
    CtrlK,
    IndexMemory,
    ExeSize,
    TraySummon,
}

/// `Bytes` shows as MB (10^6), `BytesBinary` as MiB; values are always stored in base units.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Unit {
    Micros,
    Bytes,
    BytesBinary,
    MilliPerSecond,
}

#[derive(Clone, Copy, Debug)]
pub struct TargetSpec {
    pub id: TargetId,
    pub measure: SafeText,
    pub unit: Unit,
    pub limit: u64,
    /// True for "below"; false for "at most".
    pub strict: bool,
    pub source: SafeText,
}

const fn spec(
    id: TargetId,
    measure: SafeText,
    (limit, unit, strict): (u64, Unit, bool),
    source: SafeText,
) -> TargetSpec {
    TargetSpec {
        id,
        measure,
        unit,
        limit,
        strict,
        source,
    }
}

/// The v1.0 targets, each with the document that sets it (SP13 plan, "v1.0 targets in the report").
pub const TARGETS: [TargetSpec; 13] = [
    spec(
        TargetId::ColdUsable,
        safe_text!("Cold start to first usable (v1.0)"),
        (400 * MS, Unit::Micros, false),
        safe_text!("docs/roadmap.md beta exit criteria; speed plan Targets"),
    ),
    spec(
        TargetId::ColdUsableSpeedPhase,
        safe_text!("Cold start to first usable (speed phase, laptop)"),
        (800 * MS, Unit::Micros, false),
        safe_text!("speed plan Targets"),
    ),
    spec(
        TargetId::WarmFirstFrame,
        safe_text!("Warm start, first frame"),
        (150 * MS, Unit::Micros, false),
        safe_text!("docs/roadmap.md; docs/perf.md"),
    ),
    spec(
        TargetId::WarmFirstUsable,
        safe_text!("Warm start, first usable"),
        (250 * MS, Unit::Micros, false),
        safe_text!("speed plan Targets"),
    ),
    spec(
        TargetId::IdlePrivateBytes,
        safe_text!("Private bytes, two panes, idle"),
        (80 * MB, Unit::Bytes, true),
        safe_text!("docs/roadmap.md; speed plan"),
    ),
    spec(
        TargetId::IdleWakeupsFocused,
        safe_text!("Idle wakeups per second, focused, without the busiest thread"),
        (1_000, Unit::MilliPerSecond, false),
        safe_text!("speed plan; speed-atoms D10"),
    ),
    spec(
        TargetId::IdleWakeupsMinimised,
        safe_text!("Idle wakeups per second, minimised, in total"),
        (1_000, Unit::MilliPerSecond, false),
        safe_text!("speed plan; speed-atoms D10"),
    ),
    spec(
        TargetId::KeyToFrameP95,
        safe_text!("Key-to-frame p95 (Down, PageDown, filter keys)"),
        (16 * MS, Unit::Micros, false),
        safe_text!("speed plan (two panes here; the target names 50k files and 4 panes)"),
    ),
    spec(
        TargetId::Open100k,
        safe_text!("Open a 100k-file folder (headless list, no frame)"),
        (200 * MS, Unit::Micros, false),
        safe_text!("speed plan"),
    ),
    spec(
        TargetId::CtrlK,
        safe_text!("Ctrl+K query (service latency without UI)"),
        (50 * MS, Unit::Micros, false),
        safe_text!("P40 done-when"),
    ),
    spec(
        TargetId::IndexMemory,
        safe_text!("Index service memory for 1 M files"),
        (100 * MB, Unit::Bytes, true),
        safe_text!("beta IX6"),
    ),
    spec(
        TargetId::ExeSize,
        safe_text!("Exe size"),
        (160 * MIB, Unit::BytesBinary, false),
        safe_text!("speed-atoms D9 (CI guard; D44: no hard limit)"),
    ),
    spec(
        TargetId::TraySummon,
        safe_text!("Summon from tray"),
        (50 * MS, Unit::Micros, false),
        safe_text!("speed plan; not measured in v1"),
    ),
];

/// Entries the index needs before its memory is compared with the 1 M files target.
pub const INDEX_TARGET_ENTRIES: u64 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "verdict", rename_all = "kebab-case")]
pub enum Verdict {
    Pass,
    Fail,
    NotMeasured { reason: SafeText },
}

/// A number, or the fixed reason it is missing.
pub type Observed = Result<u64, SafeText>;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct TargetRow {
    pub id: TargetId,
    pub measure: SafeText,
    pub source: SafeText,
    pub unit: Unit,
    pub limit: u64,
    pub strict: bool,
    pub value: Option<u64>,
    #[serde(flatten)]
    pub verdict: Verdict,
}

impl TargetRow {
    pub fn judge(spec: &TargetSpec, observed: Observed) -> Self {
        let (value, verdict) = match observed {
            Ok(value) if within(spec, value) => (Some(value), Verdict::Pass),
            Ok(value) => (Some(value), Verdict::Fail),
            Err(reason) => (None, Verdict::NotMeasured { reason }),
        };
        Self {
            id: spec.id,
            measure: spec.measure,
            source: spec.source,
            unit: spec.unit,
            limit: spec.limit,
            strict: spec.strict,
            value,
            verdict,
        }
    }
}

fn within(spec: &TargetSpec, value: u64) -> bool {
    if spec.strict {
        value < spec.limit
    } else {
        value <= spec.limit
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Overall {
    AllPass,
    SomeFail,
    Incomplete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Tally {
    pub pass: u32,
    pub fail: u32,
    pub not_measured: u32,
    pub overall: Overall,
}

impl Tally {
    pub fn of(rows: &[TargetRow]) -> Self {
        let count = |wanted: fn(&Verdict) -> bool| {
            let n = rows.iter().filter(|row| wanted(&row.verdict)).count();
            u32::try_from(n).unwrap_or(u32::MAX)
        };
        let pass = count(|v| matches!(v, Verdict::Pass));
        let fail = count(|v| matches!(v, Verdict::Fail));
        let not_measured = count(|v| matches!(v, Verdict::NotMeasured { .. }));
        let overall = match (fail, not_measured) {
            (0, 0) => Overall::AllPass,
            (0, _) => Overall::Incomplete,
            _ => Overall::SomeFail,
        };
        Self {
            pass,
            fail,
            not_measured,
            overall,
        }
    }
}
