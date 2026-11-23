//! Pure timer state machines: outbox (Undo Send / Send Later), snooze, and follow-ups.
//!
//! Every transition is a pure function of `(state, event, now)` so crash recovery and
//! idempotency are unit-testable without a database or clock.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::ids::{MessageId, UnixMs};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
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
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SnoozeState {
    Armed { until: UnixMs },
    Woken { at: UnixMs },
    Cancelled { at: UnixMs },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnoozeEvent {
    /// The snooze deadline passed.
    Fire,
    /// The user brought the thread back early.
    Cancel,
    /// The user snoozed again to a new time.
    Reschedule(UnixMs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnoozeEffect {
    NotifyWoken,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FollowUpState {
    Armed { due: UnixMs },
    Satisfied { by: MessageId, at: UnixMs },
    Fired { at: UnixMs },
    Dismissed { at: UnixMs },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FollowUpEvent {
    /// The follow-up deadline passed.
    Fire,
    /// The user dismissed the reminder.
    Dismiss,
    /// The user re-armed or moved the follow-up.
    Set(UnixMs),
    /// A reply was sent on the thread; the reminder is satisfied.
    Satisfy { by: MessageId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowUpEffect {
    NotifyDue,
}

/// Advance one outbox job. `Err(CoreError::TooLate)` marks the point of no return:
/// once SMTP has started, cancellation is refused so mail is never double-sent.
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
        (OutboxState::Failed { .. }, OutboxEvent::Retry) => {
            Ok((OutboxState::Scheduled { due: now }, None))
        }
        (st, ev) => Err(CoreError::IllegalState {
            reason: format!("cannot apply {ev:?} to {st:?}"),
        }),
    }
}

/// Advance one snooze job. Firing before the deadline is an illegal transition.
pub fn step_snooze(
    state: SnoozeState,
    event: SnoozeEvent,
    now: UnixMs,
) -> Result<(SnoozeState, Option<SnoozeEffect>), CoreError> {
    match (state, event) {
        (SnoozeState::Armed { until }, SnoozeEvent::Fire) if now >= until => Ok((
            SnoozeState::Woken { at: now },
            Some(SnoozeEffect::NotifyWoken),
        )),
        (st @ SnoozeState::Armed { .. }, SnoozeEvent::Fire) => Err(CoreError::IllegalState {
            reason: format!("snooze not yet due: {st:?}"),
        }),
        (SnoozeState::Armed { .. }, SnoozeEvent::Cancel) => {
            Ok((SnoozeState::Cancelled { at: now }, None))
        }
        (SnoozeState::Cancelled { at }, SnoozeEvent::Cancel) => {
            Ok((SnoozeState::Cancelled { at }, None))
        }
        (SnoozeState::Armed { .. }, SnoozeEvent::Reschedule(until)) => {
            Ok((SnoozeState::Armed { until }, None))
        }
        (st, ev) => Err(CoreError::IllegalState {
            reason: format!("cannot apply {ev:?} to {st:?}"),
        }),
    }
}

/// Advance one follow-up job. Re-firing an already-fired reminder is a no-op so
/// repeated scheduler ticks stay idempotent.
pub fn step_follow_up(
    state: FollowUpState,
    event: FollowUpEvent,
    now: UnixMs,
) -> Result<(FollowUpState, Option<FollowUpEffect>), CoreError> {
    match (state, event) {
        (FollowUpState::Armed { due }, FollowUpEvent::Fire) if now >= due => Ok((
            FollowUpState::Fired { at: now },
            Some(FollowUpEffect::NotifyDue),
        )),
        (st @ FollowUpState::Armed { .. }, FollowUpEvent::Fire) => Err(CoreError::IllegalState {
            reason: format!("follow-up not yet due: {st:?}"),
        }),
        (st @ FollowUpState::Fired { .. }, FollowUpEvent::Fire) => Ok((st, None)),
        (FollowUpState::Armed { .. }, FollowUpEvent::Dismiss)
        | (FollowUpState::Fired { .. }, FollowUpEvent::Dismiss) => {
            Ok((FollowUpState::Dismissed { at: now }, None))
        }
        (FollowUpState::Dismissed { at }, FollowUpEvent::Dismiss) => {
            Ok((FollowUpState::Dismissed { at }, None))
        }
        (FollowUpState::Armed { .. }, FollowUpEvent::Set(due))
        | (FollowUpState::Fired { .. }, FollowUpEvent::Set(due))
        | (FollowUpState::Dismissed { .. }, FollowUpEvent::Set(due)) => {
            Ok((FollowUpState::Armed { due }, None))
        }
        (st, FollowUpEvent::Satisfy { by }) => match st {
            FollowUpState::Satisfied { .. } => Ok((st, None)),
            _ => Ok((FollowUpState::Satisfied { by, at: now }, None)),
        },
        (st, ev) => Err(CoreError::IllegalState {
            reason: format!("cannot apply {ev:?} to {st:?}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: UnixMs = UnixMs(1_760_000_000_000);

    fn msg() -> MessageId {
        MessageId::fresh()
    }

    // --- Outbox: Undo Send / crash recovery ---

    #[test]
    fn scheduled_send_starts_smtp_when_due() {
        let (state, effect) =
            step_outbox(OutboxState::Scheduled { due: T0 }, OutboxEvent::Due, T0).unwrap();
        assert_eq!(effect, Some(OutboxEffect::StartSmtp));
        assert_eq!(
            state,
            OutboxState::Sending {
                attempt: 1,
                lease_until: T0.add_secs(60)
            }
        );
    }

    #[test]
    fn undo_send_before_start_cancels() {
        let (state, effect) =
            step_outbox(OutboxState::Scheduled { due: T0 }, OutboxEvent::Cancel, T0).unwrap();
        assert_eq!(state, OutboxState::Cancelled { at: T0 });
        assert_eq!(effect, None);

        // RetryWait (backing off) is also still cancellable.
        let retry = OutboxState::RetryWait {
            attempt: 2,
            due: T0.add_secs(60),
        };
        let (state, _) = step_outbox(retry, OutboxEvent::Cancel, T0).unwrap();
        assert_eq!(state, OutboxState::Cancelled { at: T0 });
    }

    #[test]
    fn undo_send_after_start_is_too_late() {
        for state in [
            OutboxState::Sending {
                attempt: 1,
                lease_until: T0.add_secs(60),
            },
            OutboxState::Verifying { attempt: 1 },
            OutboxState::Sent { at: T0 },
        ] {
            let err = step_outbox(state.clone(), OutboxEvent::Cancel, T0).unwrap_err();
            assert_eq!(
                err,
                CoreError::TooLate,
                "state {state:?} must refuse cancel"
            );
        }
    }

    #[test]
    fn crash_recovery_moves_expired_lease_to_verifying_before_any_retry() {
        // A crash left the job in Sending past its lease. The ONLY legal next step is
        // Verifying + a Sent-folder search — never a blind resend.
        let expired = OutboxState::Sending {
            attempt: 2,
            lease_until: T0.add_secs(-10),
        };
        let (state, effect) = step_outbox(expired, OutboxEvent::LeaseExpired, T0).unwrap();
        assert_eq!(state, OutboxState::Verifying { attempt: 2 });
        assert_eq!(effect, Some(OutboxEffect::SearchSent));

        // Not in Sent: back off and retry later (idempotent transition).
        let (state, effect) = step_outbox(state, OutboxEvent::NotInSent, T0).unwrap();
        assert_eq!(effect, None);
        assert!(matches!(state, OutboxState::RetryWait { attempt: 2, .. }));

        // Found in Sent: mark sent without ever re-entering SMTP.
        let (state, _) = step_outbox(
            OutboxState::Verifying { attempt: 1 },
            OutboxEvent::FoundInSent,
            T0,
        )
        .unwrap();
        assert_eq!(state, OutboxState::Sent { at: T0 });
    }

    #[test]
    fn smtp_transient_retries_until_permanent_failure() {
        let sending = OutboxState::Sending {
            attempt: 1,
            lease_until: T0.add_secs(60),
        };
        let (state, effect) = step_outbox(
            sending,
            OutboxEvent::SmtpTransient {
                reason: "connection reset".into(),
            },
            T0,
        )
        .unwrap();
        assert_eq!(effect, None);
        assert!(matches!(state, OutboxState::RetryWait { attempt: 1, .. }));

        // Fifth consecutive failure gives up.
        let (state, effect) = step_outbox(
            OutboxState::Sending {
                attempt: 5,
                lease_until: T0.add_secs(60),
            },
            OutboxEvent::SmtpTransient {
                reason: "connection reset".into(),
            },
            T0,
        )
        .unwrap();
        assert_eq!(effect, Some(OutboxEffect::NotifyFailed));
        assert!(matches!(state, OutboxState::Failed { .. }));

        // Permanent rejection fails immediately.
        let (state, effect) = step_outbox(
            OutboxState::Sending {
                attempt: 1,
                lease_until: T0.add_secs(60),
            },
            OutboxEvent::SmtpPermanent {
                reason: "550 mailbox unavailable".into(),
            },
            T0,
        )
        .unwrap();
        assert_eq!(effect, Some(OutboxEffect::NotifyFailed));
        assert!(matches!(state, OutboxState::Failed { .. }));
    }

    #[test]
    fn failed_job_can_be_retried_back_to_scheduled() {
        let (state, _) = step_outbox(
            OutboxState::Failed {
                reason: "boom".into(),
            },
            OutboxEvent::Retry,
            T0,
        )
        .unwrap();
        assert_eq!(state, OutboxState::Scheduled { due: T0 });
    }

    #[test]
    fn illegal_outbox_transitions_are_rejected() {
        let err = step_outbox(OutboxState::Sent { at: T0 }, OutboxEvent::Due, T0).unwrap_err();
        assert!(matches!(err, CoreError::IllegalState { .. }));
    }

    // --- Snooze ---

    #[test]
    fn snooze_fires_only_after_the_deadline() {
        let until = T0.add_secs(3600);
        let err = step_snooze(SnoozeState::Armed { until }, SnoozeEvent::Fire, T0).unwrap_err();
        assert!(matches!(err, CoreError::IllegalState { .. }));

        let (state, effect) =
            step_snooze(SnoozeState::Armed { until }, SnoozeEvent::Fire, until).unwrap();
        assert_eq!(state, SnoozeState::Woken { at: until });
        assert_eq!(effect, Some(SnoozeEffect::NotifyWoken));
    }

    #[test]
    fn snooze_cancel_is_idempotent() {
        let (state, _) =
            step_snooze(SnoozeState::Armed { until: T0 }, SnoozeEvent::Cancel, T0).unwrap();
        assert_eq!(state, SnoozeState::Cancelled { at: T0 });
        let (again, _) = step_snooze(state, SnoozeEvent::Cancel, T0.add_secs(5)).unwrap();
        assert_eq!(again, SnoozeState::Cancelled { at: T0 });
    }

    #[test]
    fn snooze_can_be_rescheduled_while_armed() {
        let later = T0.add_secs(7200);
        let (state, _) = step_snooze(
            SnoozeState::Armed { until: T0 },
            SnoozeEvent::Reschedule(later),
            T0,
        )
        .unwrap();
        assert_eq!(state, SnoozeState::Armed { until: later });
    }

    // --- Follow-up ---

    #[test]
    fn follow_up_fires_once_and_stays_fired() {
        let due = T0.add_secs(60);
        let err =
            step_follow_up(FollowUpState::Armed { due }, FollowUpEvent::Fire, T0).unwrap_err();
        assert!(matches!(err, CoreError::IllegalState { .. }));

        let (state, effect) =
            step_follow_up(FollowUpState::Armed { due }, FollowUpEvent::Fire, due).unwrap();
        assert_eq!(state, FollowUpState::Fired { at: due });
        assert_eq!(effect, Some(FollowUpEffect::NotifyDue));

        // Re-firing on the next tick is a no-op, never an error.
        let (again, effect) =
            step_follow_up(state.clone(), FollowUpEvent::Fire, due.add_secs(5)).unwrap();
        assert_eq!(again, state);
        assert_eq!(effect, None);
    }

    #[test]
    fn follow_up_dismiss_and_rearm() {
        let (state, _) =
            step_follow_up(FollowUpState::Fired { at: T0 }, FollowUpEvent::Dismiss, T0).unwrap();
        assert_eq!(state, FollowUpState::Dismissed { at: T0 });
        let (again, _) = step_follow_up(state, FollowUpEvent::Dismiss, T0.add_secs(1)).unwrap();
        assert_eq!(again, FollowUpState::Dismissed { at: T0 });

        let due = T0.add_secs(86_400);
        let (state, _) = step_follow_up(
            FollowUpState::Dismissed { at: T0 },
            FollowUpEvent::Set(due),
            T0,
        )
        .unwrap();
        assert_eq!(state, FollowUpState::Armed { due });
    }

    #[test]
    fn follow_up_satisfied_by_a_reply() {
        let by = msg();
        for state in [
            FollowUpState::Armed { due: T0 },
            FollowUpState::Fired { at: T0 },
        ] {
            let (next, effect) = step_follow_up(state, FollowUpEvent::Satisfy { by }, T0).unwrap();
            assert_eq!(next, FollowUpState::Satisfied { by, at: T0 });
            assert_eq!(effect, None);
        }
    }
}
