//! Phase 3 integration tests: Gmail-syntax search, saved searches, sender screener,
//! VIP split inbox, labels, and client-side auto-labeling rules.

use std::sync::Arc;

use mail_core::auth::MemorySecretStore;
use mail_core::host::ManualClock;
use mail_core::ids::UnixMs;
use mail_core::rules::{RuleAction, RuleCondition};
use mail_core::types::*;
use mail_core::{CoreConfig, Host, MailCore};

const START: i64 = 1_760_000_000_000;

async fn seeded_core() -> MailCore {
    let clock = Arc::new(ManualClock::new(UnixMs(START)));
    let core = MailCore::open(
        CoreConfig::default(),
        Host {
            secrets: Arc::new(MemorySecretStore::default()),
            clock,
        },
    )
    .expect("open core");
    core.seed_demo_if_empty().await.expect("seed");
    core
}

#[tokio::test]
async fn gmail_syntax_search_operators_negation_and_saved_searches() {
    let core = seeded_core().await;

    // `from:` + `OR` + `-negation`
    let hits = core
        .list_threads(ThreadQuery {
            scope: Scope::Unified,
            view: MailboxView::Search {
                text: "from:dana@ferngate.co OR from:billing@figma.com".into(),
            },
            limit: 50,
            offset: 0,
        })
        .await
        .expect("search OR");
    assert!(hits.items.len() >= 2);

    let negated = core
        .list_threads(ThreadQuery {
            scope: Scope::Unified,
            view: MailboxView::Search {
                text: "(from:dana@ferngate.co OR from:billing@figma.com) -from:figma.com".into(),
            },
            limit: 50,
            offset: 0,
        })
        .await
        .expect("search negated");
    assert!(negated
        .items
        .iter()
        .all(|t| !t.sender_email.contains("figma.com")));
    assert!(negated
        .items
        .iter()
        .any(|t| t.sender_email.contains("dana@ferngate.co")));

    // Saved searches CRUD
    let saved = core
        .save_saved_search(SaveSavedSearchInput {
            id: None,
            name: "Invoices".into(),
            query: "subject:invoice OR from:billing".into(),
        })
        .await
        .expect("save search");
    assert_eq!(core.list_saved_searches().await.unwrap().len(), 1);
    core.delete_saved_search(saved.id).await.expect("delete");
    assert!(core.list_saved_searches().await.unwrap().is_empty());
}

#[tokio::test]
async fn sender_screener_and_vip_split_inbox() {
    let core = seeded_core().await;

    let pending = core.list_screener_senders().await.expect("screener");
    assert!(!pending.is_empty());
    let first = pending[0].clone();

    // Screen first sender to Paper Trail (receipts)
    core.screen_sender(
        first.address.clone(),
        Some(first.display_name.clone()),
        ScreenerDecision::PaperTrail,
    )
    .await
    .expect("screen paper trail");

    let after = core.list_screener_senders().await.expect("screener after");
    assert!(after.iter().all(|s| s.address != first.address));

    // Add VIP sender & verify `is:vip` + `is_vip` badge + `MailboxCounts.vip`
    core.add_vip_sender("dana@ferngate.co".into(), Some("Dana".into()))
        .await
        .expect("add vip");
    let vips = core.list_vip_senders().await.expect("vips");
    assert_eq!(vips.len(), 1);

    let vip_hits = core
        .list_threads(ThreadQuery {
            scope: Scope::Unified,
            view: MailboxView::Search {
                text: "is:vip".into(),
            },
            limit: 20,
            offset: 0,
        })
        .await
        .expect("vip search");
    assert!(!vip_hits.items.is_empty());
    assert!(vip_hits.items.iter().all(|t| t.is_vip));

    let counts = core.mailbox_counts(Scope::Unified).await.expect("counts");
    assert!(counts.vip >= 1);

    core.remove_vip_sender("dana@ferngate.co".into())
        .await
        .expect("remove vip");
    assert!(core.list_vip_senders().await.unwrap().is_empty());
}

#[tokio::test]
async fn labels_and_client_side_auto_labeling_rules() {
    let core = seeded_core().await;

    let label = core
        .save_label(SaveLabelInput {
            id: None,
            name: "Clients".into(),
            color: SpaceColor::Emerald,
        })
        .await
        .expect("save label");

    let rule = core
        .save_rule(SaveMailRuleInput {
            id: None,
            name: "Tag Ferngate clients".into(),
            enabled: true,
            match_all: true,
            conditions: vec![RuleCondition::DomainEquals {
                value: "ferngate.co".into(),
            }],
            actions: vec![
                RuleAction::ApplyLabel { label_id: label.id },
                RuleAction::Star,
            ],
        })
        .await
        .expect("save rule");

    let matched = core.run_rule_now(rule.id).await.expect("run rule");
    assert!(matched >= 1);

    // Label view now returns the auto-labeled thread and attaches its badge!
    let labeled = core
        .list_threads(ThreadQuery {
            scope: Scope::Unified,
            view: MailboxView::Label { label_id: label.id },
            limit: 20,
            offset: 0,
        })
        .await
        .expect("label view");
    assert_eq!(labeled.items.len(), matched as usize);
    assert!(labeled.items[0].starred);
    assert!(labeled.items[0].labels.iter().any(|l| l.id == label.id));

    // Search by `label:clients` also finds it!
    let by_search = core
        .list_threads(ThreadQuery {
            scope: Scope::Unified,
            view: MailboxView::Search {
                text: "label:clients".into(),
            },
            limit: 20,
            offset: 0,
        })
        .await
        .expect("label search");
    assert_eq!(by_search.items.len(), matched as usize);

    core.delete_rule(rule.id).await.expect("delete rule");
    core.delete_label(label.id).await.expect("delete label");
}
