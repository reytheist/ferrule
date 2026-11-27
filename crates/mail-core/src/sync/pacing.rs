//! Smart sync pacing & auth circuit breaker.
//!
//! Differentiates three failure classes so background sync never hammers servers
//! or locks out accounts:
//! - **Transient network drops**: fast jittered exponential backoff (`1s -> 5m`).
//! - **Server rate limits**: slower backoff (`60s -> 1h`), honoring `Retry-After` up to 24h.
//! - **Auth failures**: circuit breaker trips immediately (`SyncState::Blocked`), halting
//!   all background retries until the user explicitly re-authenticates.

use crate::ids::UnixMs;
use crate::types::SyncState;

pub(crate) const TRANSIENT_MIN_SECS: u64 = 1;
pub(crate) const TRANSIENT_MAX_SECS: u64 = 300; // 5m
pub(crate) const RATE_LIMIT_MIN_SECS: u64 = 60; // 1m
pub(crate) const RATE_LIMIT_MAX_SECS: u64 = 3_600; // 1h
pub(crate) const RETRY_AFTER_MAX_SECS: u64 = 86_400; // 24h

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FailureKind {
    TransientNetwork,
    RateLimited { retry_after_secs: Option<u64> },
    AuthFailure,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AccountPacer {
    consecutive_transient: u32,
    consecutive_rate_limit: u32,
    auth_tripped: bool,
    next_allowed_at: Option<UnixMs>,
    last_reason: Option<String>,
}

impl AccountPacer {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn is_auth_tripped(&self) -> bool {
        self.auth_tripped
    }

    pub(crate) fn next_allowed_at(&self) -> Option<UnixMs> {
        self.next_allowed_at
    }

    pub(crate) fn can_attempt(&self, now: UnixMs) -> bool {
        if self.auth_tripped {
            return false;
        }
        match self.next_allowed_at {
            Some(at) => now >= at,
            None => true,
        }
    }

    /// Clear all failure counters on a successful sync cycle.
    pub(crate) fn record_success(&mut self) {
        self.consecutive_transient = 0;
        self.consecutive_rate_limit = 0;
        self.auth_tripped = false;
        self.next_allowed_at = None;
        self.last_reason = None;
    }

    /// Reset the auth circuit breaker after the user re-authenticates or updates credentials.
    pub(crate) fn reset_auth_breaker(&mut self) {
        self.auth_tripped = false;
        self.consecutive_transient = 0;
        self.consecutive_rate_limit = 0;
        self.next_allowed_at = None;
        self.last_reason = None;
    }

    /// Record a failure and return the resulting [`SyncState`] to persist & emit.
    ///
    /// `jitter_unit` is in `0.0..=1.0` (callers pass a time-derived or pseudo-random value;
    /// tests pass deterministic values).
    pub(crate) fn record_failure(
        &mut self,
        kind: FailureKind,
        reason: String,
        now: UnixMs,
        jitter_unit: f64,
    ) -> SyncState {
        self.last_reason = Some(reason.clone());
        match kind {
            FailureKind::AuthFailure => {
                self.auth_tripped = true;
                self.next_allowed_at = None;
                SyncState::Blocked { reason }
            }
            FailureKind::TransientNetwork => {
                self.consecutive_transient = self.consecutive_transient.saturating_add(1);
                let exp = self.consecutive_transient.saturating_sub(1).min(10);
                let base = TRANSIENT_MIN_SECS
                    .saturating_mul(1u64 << exp)
                    .min(TRANSIENT_MAX_SECS);
                let delay = apply_jitter(base, jitter_unit).clamp(TRANSIENT_MIN_SECS, TRANSIENT_MAX_SECS);
                let retry_at = now.add_secs(delay as i64);
                self.next_allowed_at = Some(retry_at);
                SyncState::Backoff {
                    retry_at,
                    attempt: self.consecutive_transient,
                    reason: Some(reason),
                }
            }
            FailureKind::RateLimited { retry_after_secs } => {
                self.consecutive_rate_limit = self.consecutive_rate_limit.saturating_add(1);
                let delay = if let Some(ra) = retry_after_secs {
                    ra.clamp(RATE_LIMIT_MIN_SECS, RETRY_AFTER_MAX_SECS)
                } else {
                    let exp = self.consecutive_rate_limit.saturating_sub(1).min(8);
                    let base = RATE_LIMIT_MIN_SECS
                        .saturating_mul(1u64 << exp)
                        .min(RATE_LIMIT_MAX_SECS);
                    apply_jitter(base, jitter_unit).clamp(RATE_LIMIT_MIN_SECS, RATE_LIMIT_MAX_SECS)
                };
                let retry_at = now.add_secs(delay as i64);
                self.next_allowed_at = Some(retry_at);
                SyncState::Backoff {
                    retry_at,
                    attempt: self.consecutive_rate_limit,
                    reason: Some(reason),
                }
            }
        }
    }
}

/// Apply ±20% jitter around `base_secs` using `unit` in `0.0..=1.0`.
fn apply_jitter(base_secs: u64, unit: f64) -> u64 {
    let clamped = unit.clamp(0.0, 1.0);
    let factor = 0.8 + (clamped * 0.4); // [0.8, 1.2]
    ((base_secs as f64) * factor).round().max(1.0) as u64
}

/// Extract an optional `Retry-After` duration (in seconds) from an error or server response.
pub(crate) fn parse_retry_after_secs(text: &str) -> Option<u64> {
    let lower = text.to_ascii_lowercase();
    for needle in ["retry-after:", "retry after ", "try again in "] {
        if let Some(pos) = lower.find(needle) {
            let rest = text[pos + needle.len()..].trim_start();
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(secs) = digits.parse::<u64>() {
                if secs > 0 {
                    return Some(secs.min(RETRY_AFTER_MAX_SECS));
                }
            }
        }
    }
    None
}

/// Classify a server error message into a pacing [`FailureKind`].
pub(crate) fn classify_error_message(message: &str, auth_phase: bool) -> FailureKind {
    let lower = message.to_ascii_lowercase();
    if auth_phase
        || lower.contains("authenticationfailed")
        || lower.contains("invalid credentials")
        || lower.contains("invalid_grant")
        || lower.contains("login failed")
        || lower.contains("application-specific password")
        || lower.contains("requires re-authentication")
    {
        return FailureKind::AuthFailure;
    }
    if lower.contains("overquota")
        || lower.contains("too many")
        || lower.contains("rate limit")
        || lower.contains("throttl")
        || lower.contains("bandwidth")
        || lower.contains("429")
        || lower.contains("retry-after")
        || lower.contains("temporary system problem")
    {
        return FailureKind::RateLimited {
            retry_after_secs: parse_retry_after_secs(message),
        };
    }
    FailureKind::TransientNetwork
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_backoff_grows_exponentially_up_to_5m_cap() {
        let mut pacer = AccountPacer::new();
        let now = UnixMs(1_000_000);
        // jitter_unit = 0.5 -> factor 1.0 (no jitter)
        let mut delays = Vec::new();
        for _ in 0..12 {
            match pacer.record_failure(FailureKind::TransientNetwork, "io drop".into(), now, 0.5) {
                SyncState::Backoff { retry_at, .. } => {
                    delays.push((retry_at.0 - now.0) / 1000);
                }
                other => panic!("expected Backoff, got {other:?}"),
            }
        }
        assert_eq!(delays[0], 1);
        assert_eq!(delays[1], 2);
        assert_eq!(delays[2], 4);
        assert_eq!(delays[3], 8);
        assert_eq!(*delays.last().unwrap(), TRANSIENT_MAX_SECS as i64);
        assert!(!pacer.can_attempt(now));
        assert!(pacer.can_attempt(now.add_secs(TRANSIENT_MAX_SECS as i64)));
    }

    #[test]
    fn rate_limit_honors_retry_after_up_to_24h_and_defaults_to_60s_to_1h() {
        let mut pacer = AccountPacer::new();
        let now = UnixMs(2_000_000);

        let s1 = pacer.record_failure(
            FailureKind::RateLimited {
                retry_after_secs: None,
            },
            "throttled".into(),
            now,
            0.5,
        );
        match s1 {
            SyncState::Backoff { retry_at, .. } => assert_eq!((retry_at.0 - now.0) / 1000, 60),
            other => panic!("expected Backoff, got {other:?}"),
        }

        let s2 = pacer.record_failure(
            FailureKind::RateLimited {
                retry_after_secs: Some(7200),
            },
            "Retry-After: 7200".into(),
            now,
            0.5,
        );
        match s2 {
            SyncState::Backoff { retry_at, .. } => assert_eq!((retry_at.0 - now.0) / 1000, 7200),
            other => panic!("expected Backoff, got {other:?}"),
        }

        // Clamped to 24h max
        let s3 = pacer.record_failure(
            FailureKind::RateLimited {
                retry_after_secs: Some(200_000),
            },
            "Retry-After: 200000".into(),
            now,
            0.5,
        );
        match s3 {
            SyncState::Backoff { retry_at, .. } => {
                assert_eq!((retry_at.0 - now.0) / 1000, RETRY_AFTER_MAX_SECS as i64)
            }
            other => panic!("expected Backoff, got {other:?}"),
        }
    }

    #[test]
    fn auth_failure_trips_circuit_breaker_until_reauth() {
        let mut pacer = AccountPacer::new();
        let now = UnixMs(3_000_000);
        let state = pacer.record_failure(
            FailureKind::AuthFailure,
            "AUTHENTICATIONFAILED".into(),
            now,
            0.5,
        );
        assert!(matches!(state, SyncState::Blocked { .. }));
        assert!(pacer.is_auth_tripped());
        assert!(!pacer.can_attempt(now.add_secs(999_999)));

        pacer.reset_auth_breaker();
        assert!(!pacer.is_auth_tripped());
        assert!(pacer.can_attempt(now));
    }

    #[test]
    fn classifies_imap_error_messages_accurately() {
        assert_eq!(
            classify_error_message("NO [AUTHENTICATIONFAILED] Invalid credentials", false),
            FailureKind::AuthFailure
        );
        assert_eq!(
            classify_error_message("NO [OVERQUOTA] Retry-After: 900", false),
            FailureKind::RateLimited {
                retry_after_secs: Some(900)
            }
        );
        assert_eq!(
            classify_error_message("connection reset by peer", false),
            FailureKind::TransientNetwork
        );
    }
}
