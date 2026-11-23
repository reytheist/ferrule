use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::ids::{MessageId, UnixMs};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OutboxState {
    Scheduled { due: UnixMs },
    Sending { attempt: u32, lease_until: UnixMs },
    Verifying { attempt: u32 },
    RetryWait { attempt: u32, due: UnixMs },
    Sent { at: UnixMs },
    Failed { reason: String },
    Cancelled { at: UnixMs },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutboxEvent {
    Cancel,
    Reschedule(UnixMs),
    Due,
    SmtpAccepted,
    SmtpTransient { reason: String },
    SmtpPermanent { reason: String },
    LeaseExpired,
    FoundInSent,
    NotInSent,
    Retry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxEffect {
    StartSmtp,
    SearchSent,
    NotifyFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SnoozeState {
    Armed { until: UnixMs },
    Woken { at: UnixMs },
    Cancelled { at: UnixMs },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FollowUpState {
    Armed { due: UnixMs },
    Satisfied { by: MessageId, at: UnixMs },
    Fired { at: UnixMs },
    Dismissed { at: UnixMs },
}

pub fn step_outbox(
    state: OutboxState,
    event: OutboxEvent,
    now: UnixMs,
) -> Result<(OutboxState, Option<OutboxEffect>), CoreError> {
    match (state, event) {
        (OutboxState::Scheduled { .. }, OutboxEvent::Cancel)
        | (OutboxState::RetryWait { .. }, OutboxEvent::Cancel) => {
            Ok((OutboxState::Cancelled { at: now }, None))
        }
        (OutboxState::Cancelled { at }, OutboxEvent::Cancel) => {
            Ok((OutboxState::Cancelled { at }, None))
        }
        (OutboxState::Sending { .. }, OutboxEvent::Cancel)
        | (OutboxState::Verifying { .. }, OutboxEvent::Cancel)
        | (OutboxState::Sent { .. }, OutboxEvent::Cancel) => Err(CoreError::TooLate),
        (OutboxState::Scheduled { .. }, OutboxEvent::Reschedule(due)) => {
            Ok((OutboxState::Scheduled { due }, None))
        }
        (OutboxState::Scheduled { .. }, OutboxEvent::Due) => Ok((
            OutboxState::Sending {
                attempt: 1,
                lease_until: now.add_secs(60),
            },
            Some(OutboxEffect::StartSmtp),
        )),
        (OutboxState::RetryWait { attempt, .. }, OutboxEvent::Due) => Ok((
            OutboxState::Sending {
                attempt: attempt.saturating_add(1),
                lease_until: now.add_secs(60),
            },
            Some(OutboxEffect::StartSmtp),
        )),
        (OutboxState::Sending { .. }, OutboxEvent::SmtpAccepted)
        | (OutboxState::Verifying { .. }, OutboxEvent::FoundInSent) => {
            Ok((OutboxState::Sent { at: now }, None))
        }
        (OutboxState::Sending { attempt, .. }, OutboxEvent::LeaseExpired) => Ok((
            OutboxState::Verifying { attempt },
            Some(OutboxEffect::SearchSent),
        )),
        (OutboxState::Verifying { attempt }, OutboxEvent::NotInSent) => {
            let backoff_secs = 30_i64.saturating_mul(1_i64 << attempt.min(5));
            Ok((
                OutboxState::RetryWait {
                    attempt,
                    due: now.add_secs(backoff_secs),
                },
                None,
            ))
        }
        (OutboxState::Sending { attempt, .. }, OutboxEvent::SmtpTransient { .. }) => {
            if attempt >= 5 {
                Ok((
                    OutboxState::Failed {
                        reason: "SMTP delivery failed after 5 attempts".to_string(),
                    },
                    Some(OutboxEffect::NotifyFailed),
                ))
            } else {
                let backoff_secs = 30_i64.saturating_mul(1_i64 << attempt.min(5));
                Ok((
                    OutboxState::RetryWait {
                        attempt,
                        due: now.add_secs(backoff_secs),
                    },
                    None,
                ))
            }
        }
        (OutboxState::Sending { .. }, OutboxEvent::SmtpPermanent { reason }) => Ok((
            OutboxState::Failed { reason },
            Some(OutboxEffect::NotifyFailed),
        )),
        (OutboxState::Failed { .. }, OutboxEvent::Retry) => Ok((
            OutboxState::Scheduled { due: now },
            None,
        )),
        (st, ev) => Err(CoreError::IllegalState {
            reason: format!("cannot apply {ev:?} to {st:?}"),
        }),
    }
}
