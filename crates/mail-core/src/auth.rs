use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::error::CoreError;
use crate::ids::AccountId;

#[derive(Clone)]
pub struct SecretString(Zeroizing<String>);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    pub fn expose(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub tls: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthClient {
    pub client_id: String,
    pub has_client_secret: bool,
}

/// Optional build-time OAuth client defaults embedded via `FERRULE_GOOGLE_CLIENT_ID`,
/// `FERRULE_GOOGLE_CLIENT_SECRET`, `FERRULE_OUTLOOK_CLIENT_ID`, and
/// `FERRULE_OUTLOOK_TENANT`. When present, users can click "Sign in with Google" or
/// "Sign in with Microsoft" directly without manually pasting client credentials.
#[derive(Debug, Clone, Default)]
pub struct BuiltInOAuthDefaults {
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<SecretString>,
    pub outlook_client_id: Option<String>,
    pub outlook_tenant: Option<String>,
}

impl BuiltInOAuthDefaults {
    pub fn from_compile_env() -> Self {
        fn clean(val: Option<&str>) -> Option<String> {
            val.map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        }
        Self {
            google_client_id: clean(option_env!("FERRULE_GOOGLE_CLIENT_ID")),
            google_client_secret: clean(option_env!("FERRULE_GOOGLE_CLIENT_SECRET"))
                .map(SecretString::new),
            outlook_client_id: clean(option_env!("FERRULE_OUTLOOK_CLIENT_ID")),
            outlook_tenant: clean(option_env!("FERRULE_OUTLOOK_TENANT")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    Gmail,
    Outlook,
    Imap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CredentialKind {
    Password,
    OAuthRefreshToken,
    OAuthClientSecret,
    AiApiKey,
    WebhookSecret,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProviderAuth {
    GmailAppPassword {
        address: String,
    },
    GmailOAuth {
        address: String,
        client: OAuthClient,
    },
    OutlookOAuth {
        address: String,
        client_id: String,
        tenant: String,
    },
    ImapPassword {
        address: String,
        username: String,
        imap: Endpoint,
        smtp: Endpoint,
    },
}

impl ProviderAuth {
    pub fn address(&self) -> &str {
        match self {
            Self::GmailAppPassword { address }
            | Self::GmailOAuth { address, .. }
            | Self::OutlookOAuth { address, .. }
            | Self::ImapPassword { address, .. } => address,
        }
    }

    pub fn provider_kind(&self) -> ProviderKind {
        match self {
            Self::GmailAppPassword { .. } | Self::GmailOAuth { .. } => ProviderKind::Gmail,
            Self::OutlookOAuth { .. } => ProviderKind::Outlook,
            Self::ImapPassword { .. } => ProviderKind::Imap,
        }
    }

    pub fn credential_kind(&self) -> CredentialKind {
        match self {
            Self::GmailAppPassword { .. } | Self::ImapPassword { .. } => CredentialKind::Password,
            Self::GmailOAuth { .. } | Self::OutlookOAuth { .. } => {
                CredentialKind::OAuthRefreshToken
            }
        }
    }

    pub fn imap_endpoint(&self) -> Endpoint {
        match self {
            Self::GmailAppPassword { .. } | Self::GmailOAuth { .. } => Endpoint {
                host: "imap.gmail.com".to_string(),
                port: 993,
                tls: true,
            },
            Self::OutlookOAuth { .. } => Endpoint {
                host: "outlook.office365.com".to_string(),
                port: 993,
                tls: true,
            },
            Self::ImapPassword { imap, .. } => imap.clone(),
        }
    }

    pub fn smtp_endpoint(&self) -> Endpoint {
        match self {
            Self::GmailAppPassword { .. } | Self::GmailOAuth { .. } => Endpoint {
                host: "smtp.gmail.com".to_string(),
                port: 587,
                tls: true,
            },
            Self::OutlookOAuth { .. } => Endpoint {
                host: "smtp-mail.outlook.com".to_string(),
                port: 587,
                tls: true,
            },
            Self::ImapPassword { smtp, .. } => smtp.clone(),
        }
    }

    pub fn username(&self) -> &str {
        match self {
            Self::ImapPassword { username, .. } => username,
            _ => self.address(),
        }
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        let addr = self.address().trim();
        if !addr.contains('@') || addr.len() < 3 {
            return Err(CoreError::InvalidInput {
                field: "address".to_string(),
                reason: "valid email address required".to_string(),
            });
        }
        if let Self::ImapPassword { imap, smtp, .. } = self {
            if imap.host.trim().is_empty() || imap.port == 0 {
                return Err(CoreError::InvalidInput {
                    field: "imap".to_string(),
                    reason: "IMAP host and port required".to_string(),
                });
            }
            if smtp.host.trim().is_empty() || smtp.port == 0 {
                return Err(CoreError::InvalidInput {
                    field: "smtp".to_string(),
                    reason: "SMTP host and port required".to_string(),
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SecretKey {
    pub account_id: AccountId,
    pub kind: CredentialKind,
}

impl SecretKey {
    pub fn ai_api_key() -> Self {
        Self {
            account_id: AccountId::derive(uuid::Uuid::NAMESPACE_URL, b"ferrule:ai-api-key"),
            kind: CredentialKind::AiApiKey,
        }
    }

    pub fn webhook_secret() -> Self {
        Self {
            account_id: AccountId::derive(uuid::Uuid::NAMESPACE_URL, b"ferrule:webhook-secret"),
            kind: CredentialKind::WebhookSecret,
        }
    }

    pub fn storage_id(&self) -> String {
        let suffix = match self.kind {
            CredentialKind::Password => "password",
            CredentialKind::OAuthRefreshToken => "oauth-refresh",
            CredentialKind::OAuthClientSecret => "oauth-client-secret",
            CredentialKind::AiApiKey => "ai-api-key",
            CredentialKind::WebhookSecret => "webhook-secret",
        };
        format!("ferrule:v1:{}:{suffix}", self.account_id)
    }
}

pub trait SecretStore: Send + Sync + 'static {
    fn put(&self, key: &SecretKey, value: &SecretString) -> Result<(), CoreError>;
    fn get(&self, key: &SecretKey) -> Result<Option<SecretString>, CoreError>;
    fn delete(&self, key: &SecretKey) -> Result<(), CoreError>;
}

#[derive(Default)]
pub struct MemorySecretStore {
    inner: Mutex<HashMap<String, String>>,
}

impl SecretStore for MemorySecretStore {
    fn put(&self, key: &SecretKey, value: &SecretString) -> Result<(), CoreError> {
        let mut guard = self.inner.lock().map_err(|_| CoreError::Internal {
            reason: "secret store lock poisoned".to_string(),
        })?;
        guard.insert(key.storage_id(), value.expose().to_string());
        Ok(())
    }

    fn get(&self, key: &SecretKey) -> Result<Option<SecretString>, CoreError> {
        let guard = self.inner.lock().map_err(|_| CoreError::Internal {
            reason: "secret store lock poisoned".to_string(),
        })?;
        Ok(guard.get(&key.storage_id()).cloned().map(SecretString::new))
    }

    fn delete(&self, key: &SecretKey) -> Result<(), CoreError> {
        let mut guard = self.inner.lock().map_err(|_| CoreError::Internal {
            reason: "secret store lock poisoned".to_string(),
        })?;
        guard.remove(&key.storage_id());
        Ok(())
    }
}
