use crate::auth::{ProviderAuth, ProviderKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dialect {
    Gmail,
    Outlook,
    Generic,
}

impl Dialect {
    pub(crate) fn from_auth(auth: &ProviderAuth) -> Self {
        match auth.provider_kind() {
            ProviderKind::Gmail => Self::Gmail,
            ProviderKind::Outlook => Self::Outlook,
            ProviderKind::Imap => Self::Generic,
        }
    }

    pub(crate) fn primary_sync_folder(self) -> &'static str {
        match self {
            Self::Gmail => "[Gmail]/All Mail",
            Self::Outlook | Self::Generic => "INBOX",
        }
    }
}
