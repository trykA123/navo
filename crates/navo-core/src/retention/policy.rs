use super::{DAY_MS, RetainedState, RetentionEntry, RetentionError, RetentionId, RetentionSummary};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "PolicyData", into = "PolicyData")]
pub struct RetentionPolicy {
    days: u8,
    max_volume_bytes: u64,
    min_free_bytes: u64,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            days: 3,
            max_volume_bytes: 1024 * 1024 * 1024,
            min_free_bytes: 256 * 1024 * 1024,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyData {
    days: u8,
    max_volume_bytes: u64,
    min_free_bytes: u64,
}

impl TryFrom<PolicyData> for RetentionPolicy {
    type Error = RetentionError;

    fn try_from(data: PolicyData) -> Result<Self, Self::Error> {
        Self::new(data.days, data.max_volume_bytes, data.min_free_bytes)
    }
}

impl From<RetentionPolicy> for PolicyData {
    fn from(policy: RetentionPolicy) -> Self {
        Self {
            days: policy.days,
            max_volume_bytes: policy.max_volume_bytes,
            min_free_bytes: policy.min_free_bytes,
        }
    }
}

impl RetentionPolicy {
    pub fn days(self) -> u8 {
        self.days
    }
    pub fn max_volume_bytes(self) -> u64 {
        self.max_volume_bytes
    }
    pub fn min_free_bytes(self) -> u64 {
        self.min_free_bytes
    }

    pub fn new(
        days: u8,
        max_volume_bytes: u64,
        min_free_bytes: u64,
    ) -> Result<Self, RetentionError> {
        if !(1..=7).contains(&days) || max_volume_bytes == 0 {
            return Err(RetentionError::Invalid(
                "retention needs 1–7 days and a positive volume cap",
            ));
        }
        Ok(Self {
            days,
            max_volume_bytes,
            min_free_bytes,
        })
    }

    pub fn expires_at(self, created_at: i64) -> Result<i64, RetentionError> {
        if created_at < 0 {
            return Err(RetentionError::Invalid("negative creation time"));
        }
        created_at
            .checked_add(i64::from(self.days) * DAY_MS)
            .ok_or(RetentionError::Overflow)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeSpace {
    pub volume: u64,
    pub free_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PurgeRequest {
    pub space: VolumeSpace,
    pub now: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurgeReason {
    Expired,
    Quota,
    LowSpace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PurgeChoice {
    pub id: RetentionId,
    pub reason: PurgeReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurgePlan {
    pub choices: Vec<PurgeChoice>,
    pub remaining_bytes: u128,
    pub quota_unmet: bool,
    pub needs_space_check: bool,
}

type Candidate<'a> = &'a RetentionSummary;

pub fn select_purge(
    entries: &[RetentionEntry],
    policy: RetentionPolicy,
    request: PurgeRequest,
) -> Result<PurgePlan, RetentionError> {
    let inventory = entries
        .iter()
        .map(RetentionSummary::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    select_purge_inventory(&inventory, policy, request)
}

pub fn select_purge_inventory(
    entries: &[RetentionSummary],
    policy: RetentionPolicy,
    request: PurgeRequest,
) -> Result<PurgePlan, RetentionError> {
    if request.now < 0 {
        return Err(RetentionError::Invalid("negative maintenance time"));
    }
    let (eligible, total) = candidates(entries, request.space.volume)?;
    let mut plan = PurgePlan {
        choices: Vec::new(),
        remaining_bytes: total,
        quota_unmet: false,
        needs_space_check: request.space.free_bytes < policy.min_free_bytes,
    };
    for entry in &eligible {
        if entry.expires_at <= request.now {
            plan.choices.push(PurgeChoice {
                id: entry.id,
                reason: PurgeReason::Expired,
            });
            plan.remaining_bytes -= u128::from(entry.bytes);
        }
    }
    fill_limits(&mut plan, &eligible, (policy, request.space));
    plan.quota_unmet = plan.remaining_bytes > u128::from(policy.max_volume_bytes);
    Ok(plan)
}

fn candidates(
    entries: &[RetentionSummary],
    volume: u64,
) -> Result<(Vec<Candidate<'_>>, u128), RetentionError> {
    let mut ids = std::collections::HashSet::new();
    let mut eligible = Vec::new();
    let mut total = 0_u128;
    for entry in entries {
        if !ids.insert(entry.id) {
            return Err(RetentionError::Invalid(
                "duplicate retention entry identifier",
            ));
        }
        if entry.volume != volume || entry.state.is_terminal() {
            continue;
        }
        total += u128::from(entry.bytes);
        if entry.state == RetainedState::Ready && !entry.pinned {
            eligible.push(entry);
        }
    }
    eligible.sort_by_key(|entry| (entry.created_at, entry.id));
    Ok((eligible, total))
}

fn fill_limits(
    plan: &mut PurgePlan,
    eligible: &[Candidate<'_>],
    (policy, space): (RetentionPolicy, VolumeSpace),
) {
    let selected: std::collections::HashSet<_> =
        plan.choices.iter().map(|choice| choice.id).collect();
    for entry in eligible {
        if selected.contains(&entry.id) {
            continue;
        }
        let reason = if plan.remaining_bytes > u128::from(policy.max_volume_bytes) {
            PurgeReason::Quota
        } else if plan.choices.is_empty() && space.free_bytes < policy.min_free_bytes {
            PurgeReason::LowSpace
        } else {
            break;
        };
        plan.choices.push(PurgeChoice {
            id: entry.id,
            reason,
        });
        plan.remaining_bytes -= u128::from(entry.bytes);
    }
}
