//! `MailCore` facade methods for Phase C — Pigment Shared Calendar,
//! Email ↔ Calendar Bridges, Meeting Docs with Recurring Carryover, and Discord Integration.

use std::time::Duration;

use crate::auth::{SecretKey, SecretString};
use crate::calendar::{
    compute_availability_slots, expand_occurrences, parse_ics, parse_quick_add,
    render_discord_recap, ActionItemStatus, ActionItemView, AddInviteToCalendarInput,
    AvailabilityQuery, AvailabilitySlot, CalendarEventKind, CalendarEventView, CalendarInviteView,
    CalendarOccurrence, CalendarRangeQuery, DiscordWebhookStatus, IcsOverlayView, MeetingDocAction,
    MeetingDocView, OverlayBusyBlock, ParsedIcsEvent, PromoteActionItemInput, RecurrenceRule,
    RsvpInviteInput, SaveCalendarEventInput, SaveIcsOverlayInput, UpdateMeetingDocInput,
};
use crate::error::CoreError;
use crate::events::CoreEvent;
use crate::ids::{ActionItemId, CalendarEventId, IcsOverlayId, UnixMs};
use crate::staff::normalize_handle;
use crate::store;
use crate::MailCore;

impl MailCore {
    // ---------------------------------------------------------------- calendar events

    pub async fn list_calendar_events(
        &self,
        query: CalendarRangeQuery,
    ) -> Result<Vec<CalendarOccurrence>, CoreError> {
        let events = self.inner.store.with_conn(store::list_all_calendar_events)?;
        Ok(expand_occurrences(&events, query.range_start, query.range_end))
    }

    pub async fn save_calendar_event(
        &self,
        input: SaveCalendarEventInput,
    ) -> Result<CalendarEventView, CoreError> {
        let ws = self.get_staff_workspace().await?;
        let actor = if ws.connected && !ws.staff_handle.is_empty() {
            ws.staff_handle.clone()
        } else {
            "staff".to_string()
        };
        let now = self.now();
        let saved = self
            .inner
            .store
            .with_conn(|conn| store::save_calendar_event(conn, &input, &actor, now))?;

        if ws.connected {
            if let Some(client) = self.remote_staff_client(&ws)? {
                if client.push_calendar_event(&saved).await.is_err() {
                    self.mark_staff_offline("Calendar event saved locally — offline");
                }
            }
        }
        self.emit(CoreEvent::CalendarChanged);
        Ok(saved)
    }

    pub async fn quick_add_calendar_event(
        &self,
        text: String,
    ) -> Result<CalendarEventView, CoreError> {
        let input = parse_quick_add(&text, self.now())?;
        self.save_calendar_event(input).await
    }

    pub async fn delete_calendar_event(&self, id: CalendarEventId) -> Result<(), CoreError> {
        let removed = self
            .inner
            .store
            .with_conn(|conn| store::delete_calendar_event(conn, id))?;
        if !removed {
            return Err(CoreError::NotFound {
                entity: "calendarEvent".to_string(),
            });
        }
        let ws = self.get_staff_workspace().await?;
        if ws.connected {
            if let Some(client) = self.remote_staff_client(&ws)? {
                let _ = client.delete_calendar_event(id).await;
            }
        }
        self.emit(CoreEvent::CalendarChanged);
        Ok(())
    }

    // ---------------------------------------------------------------- meeting docs & action items

    pub async fn get_meeting_doc(
        &self,
        event_id: CalendarEventId,
    ) -> Result<MeetingDocView, CoreError> {
        let now = self.now();
        self.inner
            .store
            .with_conn(|conn| store::get_meeting_doc(conn, event_id, now))?
            .ok_or_else(|| CoreError::NotFound {
                entity: "calendarEvent".to_string(),
            })
    }

    pub async fn update_meeting_doc(
        &self,
        input: UpdateMeetingDocInput,
    ) -> Result<MeetingDocView, CoreError> {
        let ws = self.get_staff_workspace().await?;
        let actor_handle = if ws.connected && !ws.staff_handle.is_empty() {
            ws.staff_handle.clone()
        } else {
            "staff".to_string()
        };
        let actor_name = if ws.connected && !ws.staff_name.is_empty() {
            ws.staff_name.clone()
        } else {
            "Pigment Staff".to_string()
        };
        let now = self.now();

        let updated = self.inner.store.with_conn(|conn| {
            let mut doc =
                store::get_meeting_doc(conn, input.event_id, now)?.ok_or(CoreError::NotFound {
                    entity: "calendarEvent".to_string(),
                })?;

            match input.action {
                MeetingDocAction::UpdateAgenda { agenda } => {
                    doc.agenda = agenda;
                    doc.rev = doc.rev.saturating_add(1);
                    doc.updated_at = now;
                }
                MeetingDocAction::ClaimNotesLock => {
                    if let Some(holder) = &doc.notes_editor_handle {
                        if holder != &actor_handle {
                            return Err(CoreError::Conflict {
                                current_rev: doc.rev,
                            });
                        }
                    }
                    doc.notes_editor_handle = Some(actor_handle.clone());
                    doc.notes_editor_name = Some(actor_name.clone());
                    doc.rev = doc.rev.saturating_add(1);
                    doc.updated_at = now;
                }
                MeetingDocAction::SaveNotes {
                    notes_markdown,
                    release_lock,
                } => {
                    if let Some(holder) = &doc.notes_editor_handle {
                        if holder != &actor_handle {
                            return Err(CoreError::Conflict {
                                current_rev: doc.rev,
                            });
                        }
                    }
                    doc.notes_markdown = notes_markdown;
                    if release_lock {
                        doc.notes_editor_handle = None;
                        doc.notes_editor_name = None;
                    } else {
                        doc.notes_editor_handle = Some(actor_handle.clone());
                        doc.notes_editor_name = Some(actor_name.clone());
                    }
                    doc.rev = doc.rev.saturating_add(1);
                    doc.updated_at = now;
                }
                MeetingDocAction::ReleaseNotesLock => {
                    doc.notes_editor_handle = None;
                    doc.notes_editor_name = None;
                    doc.rev = doc.rev.saturating_add(1);
                    doc.updated_at = now;
                }
            }

            store::upsert_meeting_doc_row(conn, &doc)?;
            Ok(doc)
        })?;

        if ws.connected {
            if let Some(client) = self.remote_staff_client(&ws)? {
                if client.push_meeting_doc(&updated).await.is_err() {
                    self.mark_staff_offline("Meeting doc saved locally — offline");
                }
            }
        }
        self.emit(CoreEvent::CalendarChanged);
        Ok(updated)
    }

    pub async fn promote_to_action_item(
        &self,
        input: PromoteActionItemInput,
    ) -> Result<ActionItemView, CoreError> {
        let title = input.title.trim();
        if title.is_empty() {
            return Err(CoreError::InvalidInput {
                field: "title".to_string(),
                reason: "action item title is required".to_string(),
            });
        }
        let ws = self.get_staff_workspace().await?;
        let actor = if ws.connected && !ws.staff_handle.is_empty() {
            ws.staff_handle.clone()
        } else {
            "staff".to_string()
        };
        let now = self.now();

        let (item, updated_doc) = self.inner.store.with_conn(|conn| {
            let event =
                store::get_calendar_event(conn, input.event_id)?.ok_or(CoreError::NotFound {
                    entity: "calendarEvent".to_string(),
                })?;

            let item = ActionItemView {
                id: ActionItemId::fresh(),
                title: title.to_string(),
                department: input
                    .department
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.trim_start_matches('#').to_ascii_lowercase())
                    .or(event.department.clone()),
                milestone: Some("PMS 2027".to_string()),
                assignee_handle: input
                    .assignee_handle
                    .as_deref()
                    .map(normalize_handle)
                    .filter(|h| !h.is_empty()),
                status: ActionItemStatus::Open,
                due_at: input.due_at,
                location: event.location.clone(),
                cost_note: None,
                source_event_id: Some(event.id),
                source_series_id: event.series_id.clone(),
                source_thread_id: None,
                created_by: actor.clone(),
                created_at: now,
                updated_at: now,
            };
            store::upsert_action_item_row(conn, &item)?;

            let mut updated_doc = None;
            if let Some(agenda_id) = &input.agenda_item_id {
                if let Some(mut doc) = store::get_meeting_doc(conn, input.event_id, now)? {
                    if let Some(bullet) = doc.agenda.iter_mut().find(|a| &a.id == agenda_id) {
                        bullet.promoted_action_id = Some(item.id);
                        doc.rev = doc.rev.saturating_add(1);
                        doc.updated_at = now;
                        store::upsert_meeting_doc_row(conn, &doc)?;
                        updated_doc = Some(doc);
                    }
                }
            }
            Ok((item, updated_doc))
        })?;

        if ws.connected {
            if let Some(client) = self.remote_staff_client(&ws)? {
                let _ = client.push_action_item(&item).await;
                if let Some(doc) = &updated_doc {
                    let _ = client.push_meeting_doc(doc).await;
                }
            }
        }
        self.emit(CoreEvent::CalendarChanged);
        Ok(item)
    }

    pub async fn toggle_action_item(
        &self,
        id: ActionItemId,
        status: ActionItemStatus,
    ) -> Result<ActionItemView, CoreError> {
        let now = self.now();
        let updated = self
            .inner
            .store
            .with_conn(|conn| store::update_action_item_status(conn, id, status, now))?
            .ok_or_else(|| CoreError::NotFound {
                entity: "actionItem".to_string(),
            })?;
        let ws = self.get_staff_workspace().await?;
        if ws.connected {
            if let Some(client) = self.remote_staff_client(&ws)? {
                let _ = client.push_action_item(&updated).await;
            }
        }
        self.emit(CoreEvent::CalendarChanged);
        Ok(updated)
    }

    pub async fn list_action_items(&self) -> Result<Vec<ActionItemView>, CoreError> {
        self.inner.store.with_conn(store::list_action_items)
    }

    // ---------------------------------------------------------------- email <-> calendar bridges

    pub async fn rsvp_calendar_invite(
        &self,
        input: RsvpInviteInput,
    ) -> Result<CalendarInviteView, CoreError> {
        let updated = self
            .inner
            .store
            .with_conn(|conn| store::update_invite_rsvp(conn, input.message_id, &input.uid, input.status))?
            .ok_or_else(|| CoreError::NotFound {
                entity: "calendarInvite".to_string(),
            })?;
        self.emit(CoreEvent::CalendarChanged);
        Ok(updated)
    }

    pub async fn add_invite_to_pigment_calendar(
        &self,
        input: AddInviteToCalendarInput,
    ) -> Result<CalendarEventView, CoreError> {
        let invite = self
            .inner
            .store
            .with_conn(|conn| store::get_message_invite(conn, input.message_id, &input.uid))?
            .ok_or_else(|| CoreError::NotFound {
                entity: "calendarInvite".to_string(),
            })?;

        let event = self
            .save_calendar_event(SaveCalendarEventInput {
                id: invite.added_event_id,
                series_id: None,
                title: invite.summary.clone(),
                description: invite.description.clone(),
                start_at: invite.start_at,
                end_at: invite.end_at,
                all_day: invite.all_day,
                kind: CalendarEventKind::Meeting,
                department: input.department,
                location: invite.location.clone(),
                recurrence: RecurrenceRule::None,
                discord_url: None,
                attendees: if invite.organizer_email.is_empty() {
                    Vec::new()
                } else {
                    vec![invite.organizer_email.clone()]
                },
                source_ics_uid: Some(invite.uid.clone()),
            })
            .await?;

        self.inner.store.with_conn(|conn| {
            store::link_invite_event(conn, input.message_id, &input.uid, event.id)
        })?;
        Ok(event)
    }

    // ---------------------------------------------------------------- local-only personal .ics overlays

    pub async fn list_ics_overlays(&self) -> Result<Vec<IcsOverlayView>, CoreError> {
        self.inner.store.with_conn(store::list_ics_overlays)
    }

    pub async fn save_ics_overlay(
        &self,
        input: SaveIcsOverlayInput,
    ) -> Result<IcsOverlayView, CoreError> {
        let saved = self
            .inner
            .store
            .with_conn(|conn| store::save_ics_overlay(conn, &input))?;
        self.emit(CoreEvent::CalendarChanged);
        Ok(saved)
    }

    pub async fn delete_ics_overlay(&self, id: IcsOverlayId) -> Result<(), CoreError> {
        let removed = self
            .inner
            .store
            .with_conn(|conn| store::delete_ics_overlay(conn, id))?;
        if !removed {
            return Err(CoreError::NotFound {
                entity: "icsOverlay".to_string(),
            });
        }
        self.emit(CoreEvent::CalendarChanged);
        Ok(())
    }

    pub async fn refresh_ics_overlay(&self, id: IcsOverlayId) -> Result<IcsOverlayView, CoreError> {
        let overlay = self
            .inner
            .store
            .with_conn(|conn| store::get_ics_overlay(conn, id))?
            .ok_or_else(|| CoreError::NotFound {
                entity: "icsOverlay".to_string(),
            })?;
        let now = self.now();

        if overlay.url.starts_with("local://") {
            let demo_blocks = demo_personal_overlay_events(now);
            self.inner.store.with_conn(|conn| {
                store::replace_overlay_events(conn, id, &demo_blocks, now, None)
            })?;
            self.emit(CoreEvent::CalendarChanged);
            return self
                .inner
                .store
                .with_conn(|conn| store::get_ics_overlay(conn, id))?
                .ok_or_else(|| CoreError::NotFound {
                    entity: "icsOverlay".to_string(),
                });
        }

        let fetch_url = if let Some(rest) = overlay.url.strip_prefix("webcal://") {
            format!("https://{rest}")
        } else {
            overlay.url.clone()
        };

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(12))
            .build()
            .map_err(|e| CoreError::Network {
                reason: format!("init .ics HTTP client: {e}"),
                retryable: true,
            })?;

        match client.get(&fetch_url).send().await {
            Ok(resp) if resp.status().is_success() => match resp.text().await {
                Ok(body) => {
                    let parsed = parse_ics(&body);
                    self.inner.store.with_conn(|conn| {
                        store::replace_overlay_events(conn, id, &parsed, now, None)
                    })?;
                }
                Err(e) => {
                    let msg = format!("read .ics response: {e}");
                    self.inner.store.with_conn(|conn| {
                        store::replace_overlay_events(conn, id, &[], now, Some(&msg))
                    })?;
                }
            },
            Ok(resp) => {
                let msg = format!("feed returned HTTP {}", resp.status().as_u16());
                self.inner.store.with_conn(|conn| {
                    store::replace_overlay_events(conn, id, &[], now, Some(&msg))
                })?;
            }
            Err(e) => {
                let msg = format!("feed unreachable: {e}");
                self.inner.store.with_conn(|conn| {
                    store::replace_overlay_events(conn, id, &[], now, Some(&msg))
                })?;
            }
        }

        self.emit(CoreEvent::CalendarChanged);
        self.inner
            .store
            .with_conn(|conn| store::get_ics_overlay(conn, id))?
            .ok_or_else(|| CoreError::NotFound {
                entity: "icsOverlay".to_string(),
            })
    }

    pub async fn list_overlay_busy_blocks(
        &self,
        query: CalendarRangeQuery,
    ) -> Result<Vec<OverlayBusyBlock>, CoreError> {
        self.inner
            .store
            .with_conn(|conn| store::list_overlay_busy_blocks(conn, query.range_start, query.range_end))
    }

    // ---------------------------------------------------------------- availability picker

    pub async fn find_availability_slots(
        &self,
        query: AvailabilityQuery,
    ) -> Result<Vec<AvailabilitySlot>, CoreError> {
        let (events, blocks) = self.inner.store.with_conn(|conn| {
            let evs = store::list_all_calendar_events(conn)?;
            let blks = store::list_overlay_busy_blocks(conn, query.range_start, query.range_end)?;
            Ok((evs, blks))
        })?;
        let occurrences = expand_occurrences(&events, query.range_start, query.range_end);
        let mut busy: Vec<(UnixMs, UnixMs)> = occurrences
            .iter()
            .map(|o| (o.occurrence_start, o.occurrence_end))
            .collect();
        for b in &blocks {
            busy.push((b.start_at, b.end_at));
        }
        Ok(compute_availability_slots(&query, &busy))
    }

    // ---------------------------------------------------------------- Discord integration

    pub async fn get_discord_webhook_status(&self) -> Result<DiscordWebhookStatus, CoreError> {
        let present = self
            .inner
            .host
            .secrets
            .get(&SecretKey::discord_webhook_url())?
            .is_some();
        Ok(DiscordWebhookStatus {
            configured: present,
        })
    }

    pub async fn set_discord_webhook(
        &self,
        url: Option<String>,
    ) -> Result<DiscordWebhookStatus, CoreError> {
        let cleaned = url.as_deref().map(str::trim).filter(|s| !s.is_empty());
        match cleaned {
            None => {
                self.inner
                    .host
                    .secrets
                    .delete(&SecretKey::discord_webhook_url())?;
                Ok(DiscordWebhookStatus { configured: false })
            }
            Some(raw) => {
                let valid = raw.starts_with("https://discord.com/api/webhooks/")
                    || raw.starts_with("https://discordapp.com/api/webhooks/")
                    || raw.starts_with("https://ptb.discord.com/api/webhooks/")
                    || raw.starts_with("https://canary.discord.com/api/webhooks/");
                if !valid {
                    return Err(CoreError::InvalidInput {
                        field: "webhookUrl".to_string(),
                        reason: "Discord webhook URL must start with https://discord.com/api/webhooks/"
                            .to_string(),
                    });
                }
                self.inner
                    .host
                    .secrets
                    .put(&SecretKey::discord_webhook_url(), &SecretString::new(raw))?;
                Ok(DiscordWebhookStatus { configured: true })
            }
        }
    }

    pub async fn format_meeting_recap(
        &self,
        event_id: CalendarEventId,
    ) -> Result<String, CoreError> {
        let now = self.now();
        let (event, doc) = self.inner.store.with_conn(|conn| {
            let event = store::get_calendar_event(conn, event_id)?.ok_or(CoreError::NotFound {
                entity: "calendarEvent".to_string(),
            })?;
            let doc = store::get_meeting_doc(conn, event_id, now)?.ok_or(CoreError::NotFound {
                entity: "calendarEvent".to_string(),
            })?;
            Ok((event, doc))
        })?;
        Ok(render_discord_recap(&event, &doc))
    }

    pub async fn post_meeting_recap_to_discord(
        &self,
        event_id: CalendarEventId,
        custom_markdown: Option<String>,
    ) -> Result<(), CoreError> {
        let Some(webhook) = self
            .inner
            .host
            .secrets
            .get(&SecretKey::discord_webhook_url())?
        else {
            return Err(CoreError::IllegalState {
                reason: "Configure a Discord webhook URL in Calendar → Overlays & Discord first."
                    .to_string(),
            });
        };
        let content = match custom_markdown.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(custom) => custom.to_string(),
            None => self.format_meeting_recap(event_id).await?,
        };
        // Discord message limit is 2000 chars.
        let truncated: String = content.chars().take(1990).collect();

        #[derive(serde::Serialize)]
        struct DiscordPayload {
            content: String,
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(12))
            .build()
            .map_err(|e| CoreError::Network {
                reason: format!("init Discord webhook client: {e}"),
                retryable: true,
            })?;

        let resp = client
            .post(webhook.expose())
            .json(&DiscordPayload { content: truncated })
            .send()
            .await
            .map_err(|e| CoreError::Network {
                reason: format!("Discord webhook unreachable: {e}"),
                retryable: true,
            })?;
        if !resp.status().is_success() {
            return Err(CoreError::Network {
                reason: format!("Discord webhook returned HTTP {}", resp.status().as_u16()),
                retryable: resp.status().is_server_error(),
            });
        }
        Ok(())
    }
}

fn demo_personal_overlay_events(now: UnixMs) -> Vec<ParsedIcsEvent> {
    let day_ms: i64 = 86_400_000;
    let hour_ms: i64 = 3_600_000;
    let base_day = (now.0 / day_ms) * day_ms;
    vec![
        ParsedIcsEvent {
            uid: "personal-dentist-1".to_string(),
            summary: "Busy (Personal Calendar)".to_string(),
            description: String::new(),
            location: None,
            organizer_name: None,
            organizer_email: String::new(),
            start_at: UnixMs(base_day + day_ms + 14 * hour_ms),
            end_at: UnixMs(base_day + day_ms + 15 * hour_ms),
            all_day: false,
        },
        ParsedIcsEvent {
            uid: "personal-school-2".to_string(),
            summary: "Busy (Family Pickup)".to_string(),
            description: String::new(),
            location: None,
            organizer_name: None,
            organizer_email: String::new(),
            start_at: UnixMs(base_day + 2 * day_ms + 16 * hour_ms),
            end_at: UnixMs(base_day + 2 * day_ms + 17 * hour_ms),
            all_day: false,
        },
    ]
}
