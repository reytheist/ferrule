use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ids::AccountId;

#[derive(Debug, Error, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CoreError {
    #[error("not found: {entity}")]
    NotFound { entity: String },
    #[error("invalid input for {field}: {reason}")]
    InvalidInput { field: String, reason: String },
    #[error("sender identity is locked to this thread's account")]
    IdentityLocked,
    #[error("sender identity does not belong to this account")]
    IdentityMismatch,
    #[error("operation cannot be undone because it already started")]
    TooLate,
    #[error("illegal state transition: {reason}")]
    IllegalState { reason: String },
    #[error("account requires re-authentication")]
    AuthRequired { account_id: AccountId },
    #[error("OS keychain unavailable: {reason}")]
    KeychainUnavailable { reason: String },
    #[error("network error: {reason}")]
    Network { reason: String, retryable: bool },
    #[error("draft conflict (current revision {current_rev})")]
    Conflict { current_rev: u32 },
    #[error("storage error: {reason}")]
    Storage { reason: String },
    #[error("internal error: {reason}")]
    Internal { reason: String },
}
