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
    RsvpInviteInput, SaveActionItemInput, SaveCalendarEventInput, SaveIcsOverlayInput,
    UpdateMeetingDocInput,
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
                start_time: None,
                end_time: None,
                location: event.location.clone(),
                cost_note: None,
                calendar_event_id: None,
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

    pub async fn save_action_item(
        &self,
        input: SaveActionItemInput,
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

        let (saved, synced_event) = self.inner.store.with_conn(|conn| {
            let existing = match input.id {
                Some(id) => store::get_action_item(conn, id)?,
                None => None,
            };
            let id = input.id.unwrap_or_else(ActionItemId::fresh);
            let department = input
                .department
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| s.trim_start_matches('#').to_ascii_lowercase());
            let milestone = input
                .milestone
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let assignee_handle = input
                .assignee_handle
                .as_deref()
                .map(normalize_handle)
                .filter(|h| !h.is_empty());
            let start_time = input
                .start_time
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let end_time = input
                .end_time
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let location = input
                .location
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let cost_note = input
                .cost_note
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);

            let mut item = ActionItemView {
                id,
                title: title.to_string(),
                department,
                milestone,
                assignee_handle,
                status: input.status,
                due_at: input.due_at,
                start_time,
                end_time,
                location,
                cost_note,
                calendar_event_id: existing.as_ref().and_then(|e| e.calendar_event_id),
                source_event_id: input
                    .source_event_id
                    .or_else(|| existing.as_ref().and_then(|e| e.source_event_id)),
                source_series_id: input
                    .source_series_id
                    .or_else(|| existing.as_ref().and_then(|e| e.source_series_id.clone())),
                source_thread_id: input
                    .source_thread_id
                    .or_else(|| existing.as_ref().and_then(|e| e.source_thread_id)),
                created_by: existing
                    .as_ref()
                    .map(|e| e.created_by.clone())
                    .unwrap_or_else(|| actor.clone()),
                created_at: existing.as_ref().map(|e| e.created_at).unwrap_or(now),
                updated_at: now,
            };

            let mut synced_event: Option<CalendarEventView> = None;
            if input.sync_to_calendar {
                if let Some(due) = item.due_at {
                    let cal_input = calendar_input_from_action(&item, due);
                    let ev = store::save_calendar_event(conn, &cal_input, &actor, now)?;
                    item.calendar_event_id = Some(ev.id);
                    synced_event = Some(ev);
                }
            }

            store::upsert_action_item_row(conn, &item)?;
            Ok((item, synced_event))
        })?;

        if ws.connected {
            if let Some(client) = self.remote_staff_client(&ws)? {
                if client.push_action_item(&saved).await.is_err() {
                    self.mark_staff_offline("Action item saved locally — offline");
                }
                if let Some(ev) = &synced_event {
                    let _ = client.push_calendar_event(ev).await;
                }
            }
        }
        self.emit(CoreEvent::CalendarChanged);
        Ok(saved)
    }

    pub async fn delete_action_item(&self, id: ActionItemId) -> Result<(), CoreError> {
        let removed = self
            .inner
            .store
            .with_conn(|conn| store::delete_action_item(conn, id))?;
        if !removed {
            return Err(CoreError::NotFound {
                entity: "actionItem".to_string(),
            });
        }
        let ws = self.get_staff_workspace().await?;
        if ws.connected {
            if let Some(client) = self.remote_staff_client(&ws)? {
                let _ = client.delete_action_item(id).await;
            }
        }
        self.emit(CoreEvent::CalendarChanged);
        Ok(())
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

        const MAX_ICS_BYTES: usize = 5 * 1024 * 1024;
        match client.get(&fetch_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                if resp.content_length().is_some_and(|n| n > MAX_ICS_BYTES as u64) {
                    let msg = "feed exceeds 5 MB limit".to_string();
                    self.inner.store.with_conn(|conn| {
                        store::replace_overlay_events(conn, id, &[], now, Some(&msg))
                    })?;
                } else {
                    match resp.bytes().await {
                        Ok(raw) if raw.len() > MAX_ICS_BYTES => {
                            let msg = "feed exceeds 5 MB limit".to_string();
                            self.inner.store.with_conn(|conn| {
                                store::replace_overlay_events(conn, id, &[], now, Some(&msg))
                            })?;
                        }
                        Ok(raw) => {
                            let body = String::from_utf8_lossy(&raw);
                            let parsed = parse_ics(&body);
                            self.inner.store.with_conn(|conn| {
                                store::replace_overlay_events(conn, id, &parsed, now, None)
                            })?;
                        }
                        Err(e) => {
                            let msg = format!("read .ics response: {}", e.without_url());
                            self.inner.store.with_conn(|conn| {
                                store::replace_overlay_events(conn, id, &[], now, Some(&msg))
                            })?;
                        }
                    }
                }
            }
            Ok(resp) => {
                let msg = format!("feed returned HTTP {}", resp.status().as_u16());
                self.inner.store.with_conn(|conn| {
                    store::replace_overlay_events(conn, id, &[], now, Some(&msg))
                })?;
            }
            Err(e) => {
                let msg = format!("feed unreachable: {}", e.without_url());
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
                reason: format!("Discord webhook unreachable: {}", e.without_url()),
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

pub(crate) fn calendar_input_from_action(
    item: &ActionItemView,
    due_at: UnixMs,
) -> SaveCalendarEventInput {
    let day_ms: i64 = 86_400_000;
    let base_day = (due_at.0 / day_ms) * day_ms;
    let parsed_start = item.start_time.as_deref().and_then(parse_hhmm);
    let parsed_end = item.end_time.as_deref().and_then(parse_hhmm);

    let (start_at, end_at, all_day) = match parsed_start {
        Some((sh, sm)) => {
            let start_ms = base_day + (sh as i64 * 60 + sm as i64) * 60_000;
            let end_ms = match parsed_end {
                Some((eh, em)) => {
                    let candidate = base_day + (eh as i64 * 60 + em as i64) * 60_000;
                    if candidate > start_ms {
                        candidate
                    } else {
                        start_ms + 3_600_000
                    }
                }
                None => start_ms + 3_600_000,
            };
            (UnixMs(start_ms), UnixMs(end_ms), false)
        }
        None => (UnixMs(base_day), UnixMs(base_day + day_ms), true),
    };

    let kind = if item.location.is_some() && parsed_start.is_some() {
        CalendarEventKind::RoomSchedule
    } else if item.title.to_ascii_lowercase().contains("deadline") {
        CalendarEventKind::Deadline
    } else {
        CalendarEventKind::Milestone
    };

    let mut desc_parts = Vec::new();
    if let Some(m) = &item.milestone {
        desc_parts.push(format!("Event / Milestone: {m}"));
    }
    if let Some(c) = &item.cost_note {
        desc_parts.push(format!("Cost: {c}"));
    }
    desc_parts.push("Synced from Pigment Action Items".to_string());

    SaveCalendarEventInput {
        id: item.calendar_event_id,
        series_id: None,
        title: item.title.clone(),
        description: desc_parts.join("\n"),
        start_at,
        end_at,
        all_day,
        kind,
        department: item.department.clone(),
        location: item.location.clone(),
        recurrence: RecurrenceRule::None,
        discord_url: None,
        attendees: item
            .assignee_handle
            .as_ref()
            .map(|h| vec![h.clone()])
            .unwrap_or_default(),
        source_ics_uid: None,
    }
}

pub(crate) fn parse_hhmm(raw: &str) -> Option<(u32, u32)> {
    let lower = raw.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return None;
    }
    let (body, is_pm, has_meridiem) = if let Some(b) = lower.strip_suffix("pm") {
        (b.trim(), true, true)
    } else if let Some(b) = lower.strip_suffix("am") {
        (b.trim(), false, true)
    } else {
        (lower.as_str(), false, false)
    };
    let (h_str, m_str) = match body.split_once(':') {
        Some((h, m)) => (h.trim(), m.trim()),
        None => (body, "0"),
    };
    let mut h: u32 = h_str.parse().ok()?;
    let m: u32 = m_str.parse().ok()?;
    if m >= 60 {
        return None;
    }
    if has_meridiem {
        if h == 0 || h > 12 {
            return None;
        }
        if is_pm && h != 12 {
            h += 12;
        } else if !is_pm && h == 12 {
            h = 0;
        }
    } else if h >= 24 {
        return None;
    }
    Some((h, m))
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
