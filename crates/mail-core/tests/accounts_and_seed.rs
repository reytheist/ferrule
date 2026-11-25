//! Account lifecycle: validation, secret isolation from SQLite, cascade removal,
//! demo seed idempotency, mailbox counts, and templates.

use std::path::PathBuf;
use std::sync::Arc;

use mail_core::auth::{MemorySecretStore, ProviderAuth, SecretKey, SecretStore};
use mail_core::host::{Clock, ManualClock};
use mail_core::ids::UnixMs;
use mail_core::types::*;
use mail_core::{CoreConfig, Host, MailCore};
use uuid::Uuid;

const START: i64 = 1_760_000_000_000;

fn test_clock() -> (Arc<ManualClock>, Arc<dyn Clock>) {
    let clock = Arc::new(ManualClock::new(UnixMs(START)));
    let trait_clock: Arc<dyn Clock> = clock.clone();
    (clock, trait_clock)
}

async fn seeded_core() -> MailCore {
    let (_, clock) = test_clock();
    let core = MailCore::open(
        CoreConfig::default(),
        Host {
            secrets: Arc::new(MemorySecretStore::default()),
            clock,
        },
    )
    .expect("open core");
    assert!(core.seed_demo_if_empty().await.expect("seed"));
    core
}

#[tokio::test]
async fn seed_creates_demo_data_and_is_idempotent() {
    let core = seeded_core().await;

    let accounts = core.list_accounts().await.expect("accounts");
    assert_eq!(accounts.len(), 4);
    assert_eq!(accounts[0].display_name, "Studio Gmail");
    assert_eq!(accounts[0].color, SpaceColor::Azure);
    assert_eq!(accounts[1].color, SpaceColor::Berry);
    assert_eq!(accounts[2].color, SpaceColor::Teal);
    assert_eq!(accounts[3].color, SpaceColor::Amber);
    assert!(accounts
        .iter()
        .all(|a| !a.identities.is_empty() && a.identities[0].is_primary));

    // Second seed is a no-op.
    assert!(!core.seed_demo_if_empty().await.expect("seed again"));
    assert_eq!(core.list_accounts().await.expect("accounts").len(), 4);

    let counts = core.mailbox_counts(Scope::Unified).await.expect("counts");
    // T5 is snoozed out of the inbox; T1-T4 + T6 remain.
    assert_eq!(counts.inbox, 5);
    assert_eq!(counts.primary, 2);
    assert_eq!(counts.receipts, 1);
    assert_eq!(counts.newsletters, 2);
    assert_eq!(counts.snoozed, 1);
    assert_eq!(counts.follow_ups, 1);
    assert_eq!(counts.scheduled, 1);

    let templates = core.list_templates().await.expect("templates");
    assert_eq!(templates.len(), 3);
    assert!(templates
        .iter()
        .any(|t| t.shortcut.as_deref() == Some("/invoice")));
}

#[tokio::test]
async fn seed_threads_cover_views_and_sent_mail() {
    let core = seeded_core().await;

    let inbox = core
        .list_threads(ThreadQuery {
            scope: Scope::Unified,
            view: MailboxView::Inbox {
                category: InboxCategory::All,
            },
            limit: 50,
            offset: 0,
        })
        .await
        .expect("inbox");
    assert_eq!(inbox.total, 5);
    assert!(
        !inbox
            .items
            .iter()
            .any(|t| t.subject.contains("Studio booking")),
        "snoozed thread hides from inbox"
    );

    let snoozed = core
        .list_threads(ThreadQuery {
            scope: Scope::Unified,
            view: MailboxView::Snoozed,
            limit: 50,
            offset: 0,
        })
        .await
        .expect("snoozed");
    assert_eq!(snoozed.total, 1);
    assert!(snoozed.items[0].subject.contains("Studio booking"));
    assert!(snoozed.items[0].snoozed_until.is_some());

    let scheduled = core
        .list_threads(ThreadQuery {
            scope: Scope::Unified,
            view: MailboxView::Scheduled,
            limit: 50,
            offset: 0,
        })
        .await
        .expect("scheduled");
    assert_eq!(scheduled.total, 1);
    assert!(scheduled.items[0].subject.contains("Contract redlines"));

    let drafts = core
        .list_threads(ThreadQuery {
            scope: Scope::Unified,
            view: MailboxView::Drafts,
            limit: 50,
            offset: 0,
        })
        .await
        .expect("drafts");
    assert_eq!(drafts.total, 1);

    let sent = core
        .list_threads(ThreadQuery {
            scope: Scope::Unified,
            view: MailboxView::Sent,
            limit: 50,
            offset: 0,
        })
        .await
        .expect("sent");
    assert_eq!(
        sent.total, 3,
        "threads where we replied: retainer, booking, redlines"
    );

    // Account scope filters to one space.
    let accounts = core.list_accounts().await.expect("accounts");
    let personal = accounts
        .iter()
        .find(|a| a.display_name == "Personal Gmail")
        .expect("personal account");
    let scoped = core
        .list_threads(ThreadQuery {
            scope: Scope::Account {
                account_id: personal.id,
            },
            view: MailboxView::Inbox {
                category: InboxCategory::All,
            },
            limit: 50,
            offset: 0,
        })
        .await
        .expect("scoped inbox");
    assert_eq!(scoped.total, 1);
    assert!(scoped.items[0].account_label == "Personal Gmail");
}

#[tokio::test]
async fn add_account_requires_password_and_validates_address() {
    let (_, clock) = test_clock();
    let core = MailCore::open(
        CoreConfig::default(),
        Host {
            secrets: Arc::new(MemorySecretStore::default()),
            clock,
        },
    )
    .expect("open core");

    let no_secret = core
        .add_account(NewAccountInput {
            display_name: "Broken".into(),
            color: SpaceColor::Violet,
            space_kind: SpaceKind::Personal,
            auth: ProviderAuth::GmailAppPassword {
                address: "someone@gmail.com".into(),
            },
            secret: None,
        })
        .await;
    assert!(matches!(
        no_secret,
        Err(mail_core::error::CoreError::InvalidInput { .. })
    ));

    let bad_address = core
        .add_account(NewAccountInput {
            display_name: "Broken".into(),
            color: SpaceColor::Violet,
            space_kind: SpaceKind::Personal,
            auth: ProviderAuth::GmailAppPassword {
                address: "not-an-email".into(),
            },
            secret: Some("app-password".into()),
        })
        .await;
    assert!(matches!(
        bad_address,
        Err(mail_core::error::CoreError::InvalidInput { .. })
    ));

    assert!(core.list_accounts().await.expect("accounts").is_empty());
}

#[tokio::test]
async fn secrets_never_touch_sqlite_and_remove_account_cascades() {
    const SECRET: &str = "sup3r-s3cret-app-pass-42";
    let dir = std::env::temp_dir();
    let db_path: PathBuf = dir.join(format!("ferrule-secret-test-{}.db", Uuid::now_v7()));
    let clock = Arc::new(ManualClock::new(UnixMs(START)));
    let secrets = Arc::new(MemorySecretStore::default());
    let core = MailCore::open(
        CoreConfig {
            db_path: Some(db_path.clone()),
            ..CoreConfig::default()
        },
        Host {
            secrets: secrets.clone(),
            clock: clock.clone(),
        },
    )
    .expect("open core");

    let account = core
        .add_account(NewAccountInput {
            display_name: "Secret Space".into(),
            color: SpaceColor::Teal,
            space_kind: SpaceKind::Personal,
            auth: ProviderAuth::ImapPassword {
                address: "keeper@example.com".into(),
                username: "keeper@example.com".into(),
                imap: mail_core::auth::Endpoint {
                    host: "imap.example.com".into(),
                    port: 993,
                    tls: true,
                },
                smtp: mail_core::auth::Endpoint {
                    host: "smtp.example.com".into(),
                    port: 587,
                    tls: true,
                },
            },
            secret: Some(SECRET.to_string()),
        })
        .await
        .expect("add account");

    // The secret lives in the SecretStore…
    let key = SecretKey {
        account_id: account.id,
        kind: mail_core::auth::CredentialKind::Password,
    };
    let stored = secrets.get(&key).expect("get").expect("present");
    assert_eq!(stored.expose(), SECRET);

    // …and in no SQLite file byte-stream (main db, WAL, or SHM).
    for suffix in ["", "-wal", "-shm"] {
        let mut path = db_path.clone().into_os_string();
        path.push(suffix);
        let bytes = std::fs::read(&path).unwrap_or_default();
        assert!(
            !bytes.windows(SECRET.len()).any(|w| w == SECRET.as_bytes()),
            "secret leaked into {path:?}"
        );
    }

    // Removal drops the credential first, then cascades the account away.
    core.remove_account(account.id).await.expect("remove");
    assert!(secrets.get(&key).expect("get").is_none());
    assert!(core.list_accounts().await.expect("accounts").is_empty());
    assert!(matches!(
        core.list_threads(ThreadQuery {
            scope: Scope::Account {
                account_id: account.id
            },
            view: MailboxView::Inbox {
                category: InboxCategory::All
            },
            limit: 10,
            offset: 0,
        })
        .await,
        Err(mail_core::error::CoreError::NotFound { .. })
    ));

    // The database itself keeps working after the cascade.
    core.db_health_check().expect("healthy");
    assert!(core.seed_demo_if_empty().await.expect("seed"));
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
}

#[tokio::test]
async fn template_crud_roundtrip() {
    let core = seeded_core().await;
    let created = core
        .save_template(SaveTemplateInput {
            id: None,
            name: "Pricing note".into(),
            shortcut: Some("/price".into()),
            subject: "Pricing for {project}".into(),
            body_text: "Here's the estimate for {project}.".into(),
        })
        .await
        .expect("create");
    assert_eq!(core.list_templates().await.expect("list").len(), 4);

    let updated = core
        .save_template(SaveTemplateInput {
            id: Some(created.id),
            name: "Pricing note v2".into(),
            shortcut: Some("/price".into()),
            subject: "Estimate for {project}".into(),
            body_text: "Updated estimate for {project}.".into(),
        })
        .await
        .expect("update");
    assert_eq!(updated.name, "Pricing note v2");
    assert_eq!(updated.subject, "Estimate for {project}");

    core.delete_template(created.id).await.expect("delete");
    assert_eq!(core.list_templates().await.expect("list").len(), 3);
    assert!(matches!(
        core.delete_template(created.id).await,
        Err(mail_core::error::CoreError::NotFound { .. })
    ));
}

#[test]
fn os_keychain_roundtrips_secret_and_cleans_up() {
    use mail_core::auth::{CredentialKind, SecretKey, SecretStore, SecretString};
    use mail_core::host::OsKeychain;
    use mail_core::ids::AccountId;

    let store = OsKeychain::default();
    let key = SecretKey {
        account_id: AccountId::fresh(),
        kind: CredentialKind::Password,
    };
    let expected = "ferrule-keychain-roundtrip-check";

    store
        .put(&key, &SecretString::new(expected))
        .expect("OsKeychain put");
    let fetched = store
        .get(&key)
        .expect("OsKeychain get")
        .expect("OsKeychain returned None immediately after put (mock backend active?)");
    assert_eq!(fetched.expose(), expected);

    store.delete(&key).expect("OsKeychain delete");
    assert!(store.get(&key).expect("OsKeychain get after delete").is_none());
}

#[tokio::test]
async fn gmail_app_password_strips_spaces() {
    let secrets = Arc::new(MemorySecretStore::default());
    let core = MailCore::open(
        CoreConfig::default(),
        Host {
            secrets: secrets.clone(),
            clock: Arc::new(ManualClock::new(UnixMs(1_795_000_000_000))),
        },
    )
    .expect("open");

    let account = core
        .add_account(NewAccountInput {
            display_name: "Personal".into(),
            color: SpaceColor::Azure,
            space_kind: SpaceKind::Personal,
            auth: ProviderAuth::GmailAppPassword {
                address: "person@gmail.com".into(),
            },
            secret: Some("abcd efgh ijkl mnop".into()),
        })
        .await
        .expect("add");

    let key = SecretKey {
        account_id: account.id,
        kind: mail_core::auth::CredentialKind::Password,
    };
    let stored = secrets.get(&key).expect("get").expect("present");
    assert_eq!(stored.expose(), "abcdefghijklmnop");
}

#[tokio::test]
async fn second_gmail_oauth_reuses_saved_client_secret_from_first_account() {
    use mail_core::auth::{CredentialKind, OAuthClient, SecretString};

    let secrets = Arc::new(MemorySecretStore::default());
    let core = MailCore::open(
        CoreConfig::default(),
        Host {
            secrets: secrets.clone(),
            clock: Arc::new(ManualClock::new(UnixMs(1_795_000_000_000))),
        },
    )
    .expect("open");

    let first_auth = ProviderAuth::GmailOAuth {
        address: "first@gmail.com".into(),
        client: OAuthClient {
            client_id: "shared-client.apps.googleusercontent.com".into(),
            has_client_secret: true,
        },
    };
    let first = core
        .add_account(NewAccountInput {
            display_name: "First".into(),
            color: SpaceColor::Azure,
            space_kind: SpaceKind::Personal,
            auth: first_auth,
            secret: Some("refresh-1".into()),
        })
        .await
        .expect("add first");
    secrets
        .put(
            &SecretKey {
                account_id: first.id,
                kind: CredentialKind::OAuthClientSecret,
            },
            &SecretString::new("GOCSPX-shared-secret"),
        )
        .expect("put secret");

    let second_auth = ProviderAuth::GmailOAuth {
        address: "second@gmail.com".into(),
        client: OAuthClient {
            client_id: "shared-client.apps.googleusercontent.com".into(),
            has_client_secret: false,
        },
    };
    let resolved = core
        .resolve_oauth_client_secret(&second_auth, None, None)
        .expect("resolve")
        .expect("found saved client secret");
    assert_eq!(resolved.expose(), "GOCSPX-shared-secret");

    let saved = core.saved_oauth_clients().await.expect("saved clients");
    assert_eq!(
        saved.gmail_client_id.as_deref(),
        Some("shared-client.apps.googleusercontent.com")
    );
    assert!(saved.outlook_client_id.is_none());
}

#[tokio::test]
async fn update_account_changes_space_name_and_color_and_picks_unique_colors() {
    assert_eq!(SpaceColor::pick_unique(&[]), SpaceColor::Azure);
    assert_eq!(
        SpaceColor::pick_unique(&[SpaceColor::Azure]),
        SpaceColor::Berry
    );
    assert_eq!(
        SpaceColor::pick_unique(&[SpaceColor::Azure, SpaceColor::Teal]),
        SpaceColor::Berry
    );
    assert_eq!(
        SpaceColor::pick_unique(&SpaceColor::ALL),
        SpaceColor::Azure
    );

    let core = MailCore::open(
        CoreConfig::default(),
        Host {
            secrets: Arc::new(MemorySecretStore::default()),
            clock: Arc::new(ManualClock::new(UnixMs(START))),
        },
    )
    .expect("open");

    let account = core
        .add_account(NewAccountInput {
            display_name: "reytheist".into(),
            color: SpaceColor::Azure,
            space_kind: SpaceKind::Personal,
            auth: ProviderAuth::GmailAppPassword {
                address: "reytheist@gmail.com".into(),
            },
            secret: Some("abcd efgh ijkl mnop".into()),
        })
        .await
        .expect("add");

    let updated = core
        .update_account(UpdateAccountInput {
            account_id: account.id,
            display_name: "Personal Gmail".into(),
            color: SpaceColor::Emerald,
            space_kind: None,
        })
        .await
        .expect("update");
    assert_eq!(updated.display_name, "Personal Gmail");
    assert_eq!(updated.color, SpaceColor::Emerald);
    assert_eq!(updated.space_kind, SpaceKind::Personal);

    let blank = core
        .update_account(UpdateAccountInput {
            account_id: account.id,
            display_name: "   ".into(),
            color: SpaceColor::Berry,
            space_kind: None,
        })
        .await;
    assert!(matches!(
        blank,
        Err(mail_core::error::CoreError::InvalidInput { .. })
    ));
}

#[tokio::test]
async fn pigment_staff_space_and_developer_tools_settings_persist() {
    let (_, clock) = test_clock();
    let core = MailCore::open(
        CoreConfig::default(),
        Host {
            secrets: Arc::new(MemorySecretStore::default()),
            clock,
        },
    )
    .expect("open core");

    // Developer Tools must be off by default.
    let initial = core.get_settings().await.expect("initial settings");
    assert!(!initial.developer_tools_enabled);

    let enabled = core
        .update_settings(AppSettings {
            developer_tools_enabled: true,
        })
        .await
        .expect("enable dev tools");
    assert!(enabled.developer_tools_enabled);
    assert!(core
        .get_settings()
        .await
        .expect("re-read settings")
        .developer_tools_enabled);

    // Pigment Staff (IONOS) space persists SpaceKind::PigmentStaff and can be toggled.
    let staff = core
        .add_account(NewAccountInput {
            display_name: "Pigment · Sponsors".into(),
            color: SpaceColor::Amber,
            space_kind: SpaceKind::PigmentStaff,
            auth: ProviderAuth::ImapPassword {
                address: "sponsors@pigmentminiatures.com".into(),
                username: "sponsors@pigmentminiatures.com".into(),
                imap: mail_core::auth::Endpoint {
                    host: "imap.ionos.com".into(),
                    port: 993,
                    tls: true,
                },
                smtp: mail_core::auth::Endpoint {
                    host: "smtp.ionos.com".into(),
                    port: 587,
                    tls: true,
                },
            },
            secret: Some("ionos-mailbox-pass".into()),
        })
        .await
        .expect("add staff account");
    assert_eq!(staff.space_kind, SpaceKind::PigmentStaff);

    let listed = core.list_accounts().await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].space_kind, SpaceKind::PigmentStaff);

    let switched = core
        .update_account(UpdateAccountInput {
            account_id: staff.id,
            display_name: staff.display_name.clone(),
            color: staff.color,
            space_kind: Some(SpaceKind::Personal),
        })
        .await
        .expect("switch to personal");
    assert_eq!(switched.space_kind, SpaceKind::Personal);
}
