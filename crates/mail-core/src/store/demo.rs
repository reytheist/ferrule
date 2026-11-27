//! Idempotent demo seed: four realistic accounts, six multi-message threads across
//! Primary/Receipts/Newsletters (with spy pixels and one-click unsubscribe), a due
//! follow-up, a snoozed thread, a scheduled send draft, and three starter templates.

use rusqlite::Connection;

use crate::auth::{Endpoint, OAuthClient, ProviderAuth};
use crate::error::CoreError;
use crate::ids::{JobId, MessageId, ThreadId, UnixMs};
use crate::schedule::{FollowUpState, OutboxState, SnoozeState};
use crate::store::*;
use crate::sync::smtp::{freeze_message, FreezeInput};
use crate::sync::Dialect;
use crate::types::*;

pub(crate) fn seed_demo(conn: &mut Connection, now: UnixMs) -> Result<bool, CoreError> {
    if account_count(conn)? > 0 {
        return Ok(false);
    }

    let ago = |mins: i64| UnixMs(now.0.saturating_sub(mins.saturating_mul(60_000)));
    let ahead = |mins: i64| UnixMs(now.0.saturating_add(mins.saturating_mul(60_000)));
    let idle = SyncState::Idle {
        last_ok: Some(ago(5)),
    };

    // ---------------------------------------------------------------- accounts
    let studio_auth = ProviderAuth::GmailAppPassword {
        address: "mira.studio@gmail.com".to_string(),
    };
    let studio = insert_account(
        conn,
        "Studio Gmail",
        SpaceColor::Azure,
        &studio_auth,
        &idle,
        now,
    )?;
    insert_identity(
        conn,
        studio,
        "mira.studio@gmail.com",
        Some("Mira Voss"),
        true,
    )?;

    let personal_auth = ProviderAuth::GmailOAuth {
        address: "reyth.personal@gmail.com".to_string(),
        client: OAuthClient {
            client_id: "764000000000-abcd.apps.googleusercontent.com".to_string(),
            has_client_secret: false,
        },
    };
    let personal = insert_account(
        conn,
        "Personal Gmail",
        SpaceColor::Berry,
        &personal_auth,
        &idle,
        now,
    )?;
    insert_identity(
        conn,
        personal,
        "reyth.personal@gmail.com",
        Some("Reyth"),
        true,
    )?;

    let outlook_auth = ProviderAuth::OutlookOAuth {
        address: "reyth@outlook.com".to_string(),
        client_id: "00000000-0000-4000-8000-abcdef012345".to_string(),
        tenant: "common".to_string(),
    };
    let outlook = insert_account(
        conn,
        "Outlook",
        SpaceColor::Teal,
        &outlook_auth,
        &idle,
        now,
    )?;
    insert_identity(conn, outlook, "reyth@outlook.com", Some("Reyth"), true)?;
    let outlook_ident = primary_identity(conn, outlook)?.ok_or(CoreError::NotFound {
        entity: "identity".to_string(),
    })?;

    let imap_auth = ProviderAuth::ImapPassword {
        address: "hello@paperkite.press".to_string(),
        username: "hello@paperkite.press".to_string(),
        imap: Endpoint {
            host: "mail.paperkite.press".to_string(),
            port: 993,
            tls: true,
        },
        smtp: Endpoint {
            host: "mail.paperkite.press".to_string(),
            port: 587,
            tls: true,
        },
    };
    let paperkite = insert_account(
        conn,
        "Custom IMAP",
        SpaceColor::Amber,
        &imap_auth,
        &idle,
        now,
    )?;
    insert_identity(
        conn,
        paperkite,
        "hello@paperkite.press",
        Some("Paperkite Press"),
        true,
    )?;

    // 90-day window cursors, one per account's primary sync folder.
    for (account_id, auth) in [
        (studio, &studio_auth),
        (personal, &personal_auth),
        (outlook, &outlook_auth),
        (paperkite, &imap_auth),
    ] {
        upsert_folder_cursor(
            conn,
            account_id,
            Dialect::from_auth(auth).primary_sync_folder(),
            now.sub_days(90),
            now,
        )?;
    }

    // ---------------------------------------------------------------- threads

    // T1 — Ferngate retainer conversation (Studio, Primary, unread reply).
    let t1 = upsert_thread(
        conn,
        &NewThread {
            id: ThreadId::fresh(),
            account_id: studio,
            remote_key: "thread-ferngate-retainer",
            subject: "Re: Q2 illustration retainer — invoice #4821",
            snippet: "Perfect. One more thing: the cover needs to land by the 20th…",
            sender_name: "Dana Okafor",
            sender_email: "dana@ferngate.co",
            last_at: ago(300),
            category: InboxCategory::Primary,
        },
    )?;
    insert_message(
        conn,
        &NewMessage {
            id: MessageId::fresh(),
            account_id: studio,
            thread_id: t1,
            message_id_hdr: "<ferngate-1@ferngate.co>".to_string(),
            in_reply_to: None,
            from_name: Some("Dana Okafor".to_string()),
            from_email: "dana@ferngate.co".to_string(),
            to: vec!["mira.studio@gmail.com".to_string()],
            cc: vec![],
            subject: "Re: Q2 illustration retainer — invoice #4821".to_string(),
            date: ago(4_320),
            snippet: "Attached the signed retainer for Q2 — we're excited to keep going…"
                .to_string(),
            is_read: true,
            body_text: "Hi Mira,\n\nAttached the signed retainer for Q2 — we're excited to keep going. Invoice #4821 covers April through June. Could you confirm you've got it on your side?\n\nBest,\nDana".to_string(),
            raw_html: Some("<p>Hi Mira,</p><p>Attached the signed retainer for Q2 — we're excited to keep going. Invoice <strong>#4821</strong> covers April through June.</p><p>Best,<br>Dana</p>".to_string()),
            unsub: None,
            trackers: 0,
        },
    )?;
    insert_message(
        conn,
        &NewMessage {
            id: MessageId::fresh(),
            account_id: studio,
            thread_id: t1,
            message_id_hdr: "<ferngate-2@ferngate.co>".to_string(),
            in_reply_to: Some("<ferngate-1@ferngate.co>".to_string()),
            from_name: Some("Mira Voss".to_string()),
            from_email: "mira.studio@gmail.com".to_string(),
            to: vec!["dana@ferngate.co".to_string()],
            cc: vec![],
            subject: "Re: Q2 illustration retainer — invoice #4821".to_string(),
            date: ago(2_880),
            snippet: "Got it — invoice #4821 is in the queue, payment goes out Friday…".to_string(),
            is_read: true,
            body_text: "Got it — invoice #4821 is in the queue, payment goes out with our Friday run. I'll start the cover sketches this week.\n\n— Mira".to_string(),
            raw_html: None,
            unsub: None,
            trackers: 0,
        },
    )?;
    insert_message(
        conn,
        &NewMessage {
            id: MessageId::fresh(),
            account_id: studio,
            thread_id: t1,
            message_id_hdr: "<ferngate-3@ferngate.co>".to_string(),
            in_reply_to: Some("<ferngate-2@ferngate.co>".to_string()),
            from_name: Some("Dana Okafor".to_string()),
            from_email: "dana@ferngate.co".to_string(),
            to: vec!["mira.studio@gmail.com".to_string()],
            cc: vec![],
            subject: "Re: Q2 illustration retainer — invoice #4821".to_string(),
            date: ago(300),
            snippet: "Perfect. One more thing: the cover needs to land by the 20th…".to_string(),
            is_read: false,
            body_text: "Perfect. One more thing: the cover needs to land by the 20th — can you send a rough by Friday?\n\nDana".to_string(),
            raw_html: Some("<p>Perfect. One more thing: the cover needs to land by the 20th — can you send a rough by Friday?</p><p>Dana</p>".to_string()),
            unsub: None,
            trackers: 0,
        },
    )?;

    // T2 — receipt with a 1x1 open pixel (Studio, Receipts).
    let t2 = upsert_thread(
        conn,
        &NewThread {
            id: ThreadId::fresh(),
            account_id: studio,
            remote_key: "thread-figma-receipt",
            subject: "Figma Professional — receipt for March",
            snippet: "Thanks for your payment of $45.00. Your subscription renews…",
            sender_name: "Figma Billing",
            sender_email: "billing@figma.com",
            last_at: ago(2_880),
            category: InboxCategory::Receipts,
        },
    )?;
    insert_message(
        conn,
        &NewMessage {
            id: MessageId::fresh(),
            account_id: studio,
            thread_id: t2,
            message_id_hdr: "<figma-receipt-march@figma.com>".to_string(),
            in_reply_to: None,
            from_name: Some("Figma Billing".to_string()),
            from_email: "billing@figma.com".to_string(),
            to: vec!["mira.studio@gmail.com".to_string()],
            cc: vec![],
            subject: "Figma Professional — receipt for March".to_string(),
            date: ago(2_880),
            snippet: "Thanks for your payment of $45.00. Your subscription renews…".to_string(),
            is_read: true,
            body_text: "Thanks for your payment of $45.00. Your Figma Professional subscription renews on April 3.\n\n— The Figma Team".to_string(),
            raw_html: Some("<p>Thanks for your payment of <strong>$45.00</strong>. Your Figma Professional subscription renews on April 3.</p><img src=\"https://figmail.com/beacon/open.gif?u=9f3a\" width=\"1\" height=\"1\" alt=\"\">".to_string()),
            unsub: None,
            trackers: 1,
        },
    )?;

    // T3 — newsletter packed with spy pixels + one-click unsubscribe (Studio).
    let t3 = upsert_thread(
        conn,
        &NewThread {
            id: ThreadId::fresh(),
            account_id: studio,
            remote_key: "thread-dispatch-142",
            subject: "Design Dispatch #142: Type that breathes",
            snippet: "Issue #142 — variable fonts in production, plus a café pop-up…",
            sender_name: "Design Dispatch",
            sender_email: "hello@dispatch.design",
            last_at: ago(1_800),
            category: InboxCategory::Newsletters,
        },
    )?;
    insert_message(
        conn,
        &NewMessage {
            id: MessageId::fresh(),
            account_id: studio,
            thread_id: t3,
            message_id_hdr: "<dispatch-142@dispatch.design>".to_string(),
            in_reply_to: None,
            from_name: Some("Design Dispatch".to_string()),
            from_email: "hello@dispatch.design".to_string(),
            to: vec!["mira.studio@gmail.com".to_string()],
            cc: vec![],
            subject: "Design Dispatch #142: Type that breathes".to_string(),
            date: ago(1_800),
            snippet: "Issue #142 — variable fonts in production, plus a café pop-up…".to_string(),
            is_read: false,
            body_text: "Issue #142 — Type that breathes\n\nVariable fonts in production, a café pop-up type specimen in Lisbon, and five typefaces we can't stop using.\n\nRead the full issue online.".to_string(),
            raw_html: Some(r#"<html><body style="margin:0;background:#f4f1ea"><div style="max-width:600px;margin:0 auto;padding:24px;font-family:Georgia,serif"><h1 style="color:#1c1917">Type that breathes</h1><p>Variable fonts in production, a caf&eacute; pop-up type specimen in Lisbon, and five typefaces we can't stop using.</p><img src="https://cdn.dispatch.design/hero-142.jpg" width="600" height="320" alt="specimen"><img src="https://gallery.mailchimp.com/7f3a/images/1x1.gif" width="1" height="1" alt=""><img src="https://open.dispatch.design/o.gif?u=9f3a&amp;m=142" alt=""><img src="https://analytics.dispatch.design/h.gif" style="display:none" alt=""><div style="background-image:url('https://track.dispatch.design/pixel.gif?r=142');width:1px;height:1px"></div><p><a href="https://dispatch.design/issues/142">Read online</a></p></div></body></html>"#.to_string()),
            unsub: Some(UnsubscribeInfo {
                one_click: true,
                target: "https://dispatch.design/unsubscribe?u=9f3a2".to_string(),
            }),
            trackers: 4,
        },
    )?;

    // T4 — personal newsletter with klaviyo/hubspot pixels (Personal).
    let t4 = upsert_thread(
        conn,
        &NewThread {
            id: ThreadId::fresh(),
            account_id: personal,
            remote_key: "thread-northwind-fall",
            subject: "Fall layers are here — 20% off this weekend",
            snippet: "Our café pop-up returns to Portland this weekend — and layers…",
            sender_name: "Northwind Outfitters",
            sender_email: "sales@northwindoutfitters.com",
            last_at: ago(1_200),
            category: InboxCategory::Newsletters,
        },
    )?;
    insert_message(
        conn,
        &NewMessage {
            id: MessageId::fresh(),
            account_id: personal,
            thread_id: t4,
            message_id_hdr: "<northwind-fall-2025@northwindoutfitters.com>".to_string(),
            in_reply_to: None,
            from_name: Some("Northwind Outfitters".to_string()),
            from_email: "sales@northwindoutfitters.com".to_string(),
            to: vec!["reyth.personal@gmail.com".to_string()],
            cc: vec![],
            subject: "Fall layers are here — 20% off this weekend".to_string(),
            date: ago(1_200),
            snippet: "Our café pop-up returns to Portland this weekend — and layers…".to_string(),
            is_read: false,
            body_text: "Fall layers are here — 20% off through Sunday.\n\nOur café pop-up returns to Portland this weekend with the full field jacket run.".to_string(),
            raw_html: Some(r#"<div style="font-family:Helvetica,Arial,sans-serif"><h2>Fall layers are here</h2><p>Our caf&eacute; pop-up returns to Portland this weekend with the full field jacket run — 20% off through Sunday.</p><img src="https://static.klaviyo.com/ae.gif?ak=9f3a" width="1" height="1" alt=""><img src="https://track.northwindoutfitters.com/open/px.gif" width="1" height="1" alt=""><img src="https://ci.hubspot.net/collect.gif" style="visibility:hidden" alt=""><img src="https://cdn.northwindoutfitters.com/fall-hero.jpg" width="600" alt="Fall layers"><p><a href="https://northwindoutfitters.com/fall">Shop the drop</a></p></div>"#.to_string()),
            unsub: Some(UnsubscribeInfo {
                one_click: false,
                target: "mailto:unsubscribe@northwindoutfitters.com".to_string(),
            }),
            trackers: 3,
        },
    )?;

    // T5 — booking thread that is currently snoozed (Studio, Primary).
    let t5 = upsert_thread(
        conn,
        &NewThread {
            id: ThreadId::fresh(),
            account_id: studio,
            remote_key: "thread-brightanvil-booking",
            subject: "Re: Studio booking for October",
            snippet: "October 6 works — I'll hold Monday and Tuesday for you.",
            sender_name: "Mira Voss",
            sender_email: "mira.studio@gmail.com",
            last_at: ago(1_560),
            category: InboxCategory::Primary,
        },
    )?;
    insert_message(
        conn,
        &NewMessage {
            id: MessageId::fresh(),
            account_id: studio,
            thread_id: t5,
            message_id_hdr: "<brightanvil-booking-1@brightanvil.co>".to_string(),
            in_reply_to: None,
            from_name: Some("Jules Maren".to_string()),
            from_email: "jules@brightanvil.co".to_string(),
            to: vec!["mira.studio@gmail.com".to_string()],
            cc: vec![],
            subject: "Re: Studio booking for October".to_string(),
            date: ago(1_620),
            snippet: "Is the studio free the week of October 6? We'd need it…".to_string(),
            is_read: true,
            body_text: "Is the studio free the week of October 6? We'd need it for a two-day shoot plus a half-day of product stills.".to_string(),
            raw_html: None,
            unsub: None,
            trackers: 0,
        },
    )?;
    insert_message(
        conn,
        &NewMessage {
            id: MessageId::fresh(),
            account_id: studio,
            thread_id: t5,
            message_id_hdr: "<brightanvil-booking-2@brightanvil.co>".to_string(),
            in_reply_to: Some("<brightanvil-booking-1@brightanvil.co>".to_string()),
            from_name: Some("Mira Voss".to_string()),
            from_email: "mira.studio@gmail.com".to_string(),
            to: vec!["jules@brightanvil.co".to_string()],
            cc: vec![],
            subject: "Re: Studio booking for October".to_string(),
            date: ago(1_560),
            snippet: "October 6 works — I'll hold Monday and Tuesday for you.".to_string(),
            is_read: true,
            body_text: "October 6 works — I'll hold Monday and Tuesday for you. Sending a short agreement tomorrow.".to_string(),
            raw_html: None,
            unsub: None,
            trackers: 0,
        },
    )?;

    // T6 — contract redlines with a scheduled reply (Outlook, Primary).
    let t6 = upsert_thread(
        conn,
        &NewThread {
            id: ThreadId::fresh(),
            account_id: outlook,
            remote_key: "thread-northstar-redlines",
            subject: "Re: Contract redlines",
            snippet: "25% works. If you can confirm tonight, we can circulate…",
            sender_name: "Ansel Ward",
            sender_email: "ansel@northstar-legal.com",
            last_at: ago(360),
            category: InboxCategory::Primary,
        },
    )?;
    insert_message(
        conn,
        &NewMessage {
            id: MessageId::fresh(),
            account_id: outlook,
            thread_id: t6,
            message_id_hdr: "<northstar-1@northstar-legal.com>".to_string(),
            in_reply_to: None,
            from_name: Some("Ansel Ward".to_string()),
            from_email: "ansel@northstar-legal.com".to_string(),
            to: vec!["reyth@outlook.com".to_string()],
            cc: vec![],
            subject: "Re: Contract redlines".to_string(),
            date: ago(2_880),
            snippet: "Redlines attached — mostly timing language. Page 3 still…".to_string(),
            is_read: true,
            body_text: "Redlines attached — mostly timing language. Page 3 still needs your call on the kill fee.".to_string(),
            raw_html: None,
            unsub: None,
            trackers: 0,
        },
    )?;
    insert_message(
        conn,
        &NewMessage {
            id: MessageId::fresh(),
            account_id: outlook,
            thread_id: t6,
            message_id_hdr: "<northstar-2@northstar-legal.com>".to_string(),
            in_reply_to: Some("<northstar-1@northstar-legal.com>".to_string()),
            from_name: Some("Reyth".to_string()),
            from_email: "reyth@outlook.com".to_string(),
            to: vec!["ansel@northstar-legal.com".to_string()],
            cc: vec![],
            subject: "Re: Contract redlines".to_string(),
            date: ago(1_440),
            snippet: "Looking now. I'm fine with the kill fee at 25%…".to_string(),
            is_read: true,
            body_text: "Looking now. I'm fine with the kill fee at 25%; the rest matches what we discussed.".to_string(),
            raw_html: None,
            unsub: None,
            trackers: 0,
        },
    )?;
    insert_message(
        conn,
        &NewMessage {
            id: MessageId::fresh(),
            account_id: outlook,
            thread_id: t6,
            message_id_hdr: "<northstar-3@northstar-legal.com>".to_string(),
            in_reply_to: Some("<northstar-2@northstar-legal.com>".to_string()),
            from_name: Some("Ansel Ward".to_string()),
            from_email: "ansel@northstar-legal.com".to_string(),
            to: vec!["reyth@outlook.com".to_string()],
            cc: vec![],
            subject: "Re: Contract redlines".to_string(),
            date: ago(360),
            snippet: "25% works. If you can confirm tonight, we can circulate…".to_string(),
            is_read: false,
            body_text: "25% works. If you can confirm tonight, we can circulate for signature tomorrow morning.".to_string(),
            raw_html: None,
            unsub: None,
            trackers: 0,
        },
    )?;

    // ---------------------------------------------------------------- timers

    // Due follow-up on T1 (already past due — fires on the next scheduler tick).
    insert_job(
        conn,
        &NewJob {
            id: JobId::fresh(),
            kind: JobKind::FollowUp,
            account_id: studio,
            thread_id: Some(t1),
            draft_id: None,
            due_at: ago(10),
            live: true,
            state_json: to_json(&FollowUpState::Armed { due: ago(10) })?,
            summary: "Ask Dana about the cover deadline".to_string(),
            payload: None,
            message_id_hdr: None,
            created_at: ago(1_440),
        },
    )?;

    // Snoozed thread (hidden from the inbox until it wakes).
    insert_job(
        conn,
        &NewJob {
            id: JobId::fresh(),
            kind: JobKind::Snooze,
            account_id: studio,
            thread_id: Some(t5),
            draft_id: None,
            due_at: ahead(4_320),
            live: true,
            state_json: to_json(&SnoozeState::Armed {
                until: ahead(4_320),
            })?,
            summary: "Snoozed until the agreement lands".to_string(),
            payload: None,
            message_id_hdr: None,
            created_at: ago(1_500),
        },
    )?;

    // Scheduled send draft (Send Later, due in two hours).
    let frozen = freeze_message(
        FreezeInput {
            from_name: Some("Reyth".to_string()),
            from_address: outlook_ident.address.clone(),
            to: vec!["ansel@northstar-legal.com".to_string()],
            cc: vec![],
            bcc: vec![],
            subject: "Re: Contract redlines".to_string(),
            body_text:
                "Confirmed — 25% kill fee works. Circulate for signature whenever you're ready."
                    .to_string(),
            body_html: None,
            attachments: vec![],
            in_reply_to: Some("<northstar-3@northstar-legal.com>".to_string()),
            references: Some("<northstar-3@northstar-legal.com>".to_string()),
        },
        now,
    );
    let send_job_id = JobId::fresh();
    let draft_id = insert_draft(
        conn,
        &InsertDraft {
            account_id: outlook,
            from_identity_id: outlook_ident.id,
            from_locked: true,
            reply_to_thread: Some(t6),
            to: vec!["ansel@northstar-legal.com".to_string()],
            cc: vec![],
            bcc: vec![],
            subject: "Re: Contract redlines".to_string(),
            body_text:
                "Confirmed — 25% kill fee works. Circulate for signature whenever you're ready."
                    .to_string(),
            body_html: None,
            origin: DraftOrigin::Human,
            state: DraftState::Queued {
                job_id: send_job_id,
            },
            updated_at: ago(30),
        },
    )?;
    insert_job(
        conn,
        &NewJob {
            id: send_job_id,
            kind: JobKind::Send,
            account_id: outlook,
            thread_id: Some(t6),
            draft_id: Some(draft_id),
            due_at: ahead(120),
            live: true,
            state_json: to_json(&OutboxState::Scheduled { due: ahead(120) })?,
            summary: "Send Later: Re: Contract redlines".to_string(),
            payload: Some(to_json(&frozen)?),
            message_id_hdr: Some(frozen.message_id.clone()),
            created_at: ago(30),
        },
    )?;

    // ---------------------------------------------------------------- templates
    for (name, shortcut, subject, body) in [
        (
            "Invoice reminder",
            "/invoice",
            "Invoice {number} — friendly reminder",
            "Hi {name},\n\nQuick nudge on invoice {number} — happy to resend it or split it into two payments if that's easier.\n\nThanks!\n",
        ),
        (
            "Intro reply",
            "/intro",
            "Re: Introduction",
            "Hi {name},\n\nGreat to meet you — {referral} filled me in on the project. I'll follow up with next steps and a couple of dates by Friday.\n\nBest,\n",
        ),
        (
            "Out of office",
            "/ooo",
            "Out of office",
            "Thanks for your note — I'm away from email until {date} and will reply as soon as I'm back.\n\nFor anything urgent, reach the studio at studio@brightlark.example.\n",
        ),
    ] {
        let id = crate::ids::TemplateId::fresh();
        conn.execute(
            "INSERT INTO template (id, name, shortcut, subject, body_text) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                id_to_blob(id.as_bytes()),
                name,
                shortcut,
                subject,
                body
            ],
        )
        .sq()?;
    }

    Ok(true)
}
