use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::CoreError;

macro_rules! define_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn fresh() -> Self {
                Self(Uuid::now_v7())
            }

            pub fn derive(namespace: Uuid, bytes: &[u8]) -> Self {
                Self(Uuid::new_v5(&namespace, bytes))
            }

            pub fn as_uuid(self) -> Uuid {
                self.0
            }

            pub fn as_bytes(&self) -> &[u8; 16] {
                self.0.as_bytes()
            }

            pub fn from_bytes(bytes: [u8; 16]) -> Self {
                Self(Uuid::from_bytes(bytes))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = CoreError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s)
                    .map(Self)
                    .map_err(|_| CoreError::InvalidInput {
                        field: stringify!($name).to_string(),
                        reason: format!("invalid UUID: {s}"),
                    })
            }
        }
    };
}

define_id!(AccountId);
define_id!(IdentityId);
define_id!(LabelId);
define_id!(ThreadId);
define_id!(MessageId);
define_id!(AttachmentId);
define_id!(DraftId);
define_id!(JobId);
define_id!(TemplateId);
define_id!(SignatureId);
define_id!(OfflineActionId);
define_id!(SavedSearchId);
define_id!(RuleId);
define_id!(CalendarEventId);
define_id!(IcsOverlayId);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentId(String);

impl AgentId {
    pub fn parse(raw: &str) -> Result<Self, CoreError> {
        let trimmed = raw.trim();
        let valid = !trimmed.is_empty()
            && trimmed.len() <= 48
            && trimmed
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
        if !valid {
            return Err(CoreError::InvalidInput {
                field: "agentId".to_string(),
                reason: "agent id must be 1..48 chars of [a-z0-9_-]".to_string(),
            });
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnixMs(pub i64);

impl UnixMs {
    pub const fn add_secs(self, secs: i64) -> Self {
        Self(self.0.saturating_add(secs.saturating_mul(1000)))
    }

    pub const fn sub_days(self, days: i64) -> Self {
        Self(self.0.saturating_sub(days.saturating_mul(86_400_000)))
    }
}
