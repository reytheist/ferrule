//! `MailCore` facade methods for personal Calendar, Email ↔ Calendar Bridges,
//! and local-only Personal .ics Overlays.

use std::net::IpAddr;
use std::time::Duration;

use url::{Host, Url};

use crate::calendar::{
    compute_availability_slots, expand_occurrences, parse_ics, parse_quick_add,
    AddInviteToCalendarInput, AvailabilityQuery, AvailabilitySlot, CalendarEventKind,
    CalendarEventView, CalendarInviteView, CalendarOccurrence, CalendarRangeQuery, IcsOverlayView,
    OverlayBusyBlock, ParsedIcsEvent, RecurrenceRule, RsvpInviteInput, SaveCalendarEventInput,
    SaveIcsOverlayInput,
};
use crate::error::CoreError;
use crate::events::CoreEvent;
use crate::ids::{CalendarEventId, IcsOverlayId, UnixMs};
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
        let now = self.now();
        let saved = self
            .inner
            .store
            .with_conn(|conn| store::save_calendar_event(conn, &input, "me", now))?;
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

    pub async fn add_invite_to_calendar(
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
        validate_ics_overlay_url(&input.url)?;
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

        let fetch_url = match validate_ics_overlay_url(&overlay.url) {
            Ok(None) => {
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
            Ok(Some(url)) => url,
            Err(err) => {
                let msg = err.to_string();
                self.inner.store.with_conn(|conn| {
                    store::replace_overlay_events(conn, id, &[], now, Some(&msg))
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
        };

        if let Err(msg) = verify_ics_dns_public(&fetch_url).await {
            self.inner.store.with_conn(|conn| {
                store::replace_overlay_events(conn, id, &[], now, Some(&msg))
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

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(12))
            .redirect(reqwest::redirect::Policy::none())
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
                    match read_response_capped(resp, MAX_ICS_BYTES).await {
                        Ok(raw) => {
                            let body = String::from_utf8_lossy(&raw);
                            let parsed = parse_ics(&body);
                            self.inner.store.with_conn(|conn| {
                                store::replace_overlay_events(conn, id, &parsed, now, None)
                            })?;
                        }
                        Err(ReadCappedError::TooLarge) => {
                            let msg = "feed exceeds 5 MB limit".to_string();
                            self.inner.store.with_conn(|conn| {
                                store::replace_overlay_events(conn, id, &[], now, Some(&msg))
                            })?;
                        }
                        Err(ReadCappedError::Http(e)) => {
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

/// Validate an `.ics` overlay URL against SSRF and disallowed schemes.
///
/// Returns `Ok(None)` for built-in `local://` demo overlays, `Ok(Some(https_url))`
/// for allowed `https://` or `webcal://` (rewritten to `https://`) URLs pointing at
/// non-private public hosts, or `Err(CoreError::InvalidInput)` otherwise.
pub(crate) fn validate_ics_overlay_url(raw: &str) -> Result<Option<String>, CoreError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "url".to_string(),
            reason: "feed URL is required".to_string(),
        });
    }
    if trimmed
        .get(..8)
        .is_some_and(|p| p.eq_ignore_ascii_case("local://"))
    {
        return Ok(None);
    }

    let candidate = if trimmed
        .get(..9)
        .is_some_and(|p| p.eq_ignore_ascii_case("webcal://"))
    {
        format!("https://{}", &trimmed[9..])
    } else {
        trimmed.to_string()
    };

    let parsed = Url::parse(&candidate).map_err(|e| CoreError::InvalidInput {
        field: "url".to_string(),
        reason: format!("invalid feed URL: {e}"),
    })?;

    if parsed.scheme() != "https" {
        return Err(CoreError::InvalidInput {
            field: "url".to_string(),
            reason: "feed URL must use https://, webcal://, or local://".to_string(),
        });
    }

    let Some(host) = parsed.host() else {
        return Err(CoreError::InvalidInput {
            field: "url".to_string(),
            reason: "feed URL must include a host".to_string(),
        });
    };

    match host {
        Host::Ipv4(v4) => {
            if !is_public_ip(IpAddr::V4(v4)) {
                return Err(CoreError::InvalidInput {
                    field: "url".to_string(),
                    reason: "feed URL cannot target loopback, private, link-local, or multicast IP addresses".to_string(),
                });
            }
        }
        Host::Ipv6(v6) => {
            if !is_public_ip(IpAddr::V6(v6)) {
                return Err(CoreError::InvalidInput {
                    field: "url".to_string(),
                    reason: "feed URL cannot target loopback, private, link-local, or multicast IP addresses".to_string(),
                });
            }
        }
        Host::Domain(domain) => {
            let lower = domain.trim_end_matches('.').to_ascii_lowercase();
            if lower.is_empty()
                || lower == "localhost"
                || lower.ends_with(".localhost")
                || lower.ends_with(".local")
                || lower.ends_with(".internal")
            {
                return Err(CoreError::InvalidInput {
                    field: "url".to_string(),
                    reason: "feed URL cannot target localhost or internal hostnames".to_string(),
                });
            }
            if let Ok(ip) = lower.parse::<IpAddr>() {
                if !is_public_ip(ip) {
                    return Err(CoreError::InvalidInput {
                        field: "url".to_string(),
                        reason: "feed URL cannot target loopback, private, link-local, or multicast IP addresses".to_string(),
                    });
                }
            }
        }
    }

    Ok(Some(parsed.to_string()))
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            let is_cgnat = octets[0] == 100 && (octets[1] & 0b1100_0000) == 64;
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_multicast()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || is_cgnat)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(v4));
            }
            let seg0 = v6.segments()[0];
            let is_ula = (seg0 & 0xfe00) == 0xfc00;
            let is_link_local = (seg0 & 0xffc0) == 0xfe80;
            !(v6.is_loopback()
                || v6.is_multicast()
                || v6.is_unspecified()
                || is_ula
                || is_link_local)
        }
    }
}

async fn verify_ics_dns_public(fetch_url: &str) -> Result<(), String> {
    let parsed = Url::parse(fetch_url).map_err(|e| format!("invalid feed URL: {e}"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| "feed URL missing host".to_string())?;
    let port = parsed.port_or_known_default().unwrap_or(443);
    let addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| format!("resolve feed host: {e}"))?;
    for addr in addrs {
        if !is_public_ip(addr.ip()) {
            return Err("feed host resolves to a disallowed private or local address".to_string());
        }
    }
    Ok(())
}

#[derive(Debug)]
enum ReadCappedError {
    TooLarge,
    Http(reqwest::Error),
}

async fn read_response_capped(
    mut resp: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, ReadCappedError> {
    let mut buf = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(ReadCappedError::Http)? {
        if buf.len().saturating_add(chunk.len()) > max_bytes {
            return Err(ReadCappedError::TooLarge);
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn ics_overlay_url_validation_allows_safe_and_rejects_ssrf_targets() {
        assert_eq!(
            validate_ics_overlay_url("local://personal-demo").expect("local allowed"),
            None
        );
        assert_eq!(
            validate_ics_overlay_url("https://calendar.example.com/feed.ics").expect("https"),
            Some("https://calendar.example.com/feed.ics".to_string())
        );
        assert_eq!(
            validate_ics_overlay_url("webcal://calendar.example.com/feed.ics").expect("webcal"),
            Some("https://calendar.example.com/feed.ics".to_string())
        );

        for bad in [
            "http://calendar.example.com/feed.ics",
            "file:///etc/passwd",
            "ftp://calendar.example.com/feed.ics",
            "data:text/calendar,BEGIN:VCALENDAR",
            "https://127.0.0.1/feed.ics",
            "https://localhost/feed.ics",
            "https://sub.localhost/feed.ics",
            "https://169.254.169.254/latest/meta-data/",
            "https://10.0.0.5/feed.ics",
            "https://172.16.0.1/feed.ics",
            "https://192.168.1.10/feed.ics",
            "https://224.0.0.1/feed.ics",
            "https://[::1]/feed.ics",
            "https://[fc00::1]/feed.ics",
            "https://[fe80::1]/feed.ics",
            "https://[::ffff:127.0.0.1]/feed.ics",
        ] {
            assert!(
                validate_ics_overlay_url(bad).is_err(),
                "expected '{bad}' to be rejected"
            );
        }
    }

    #[tokio::test]
    async fn read_response_capped_aborts_oversized_chunked_stream() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        let server = tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut req = [0u8; 512];
                let _ = sock.read(&mut req).await;
                let _ = sock
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: text/calendar\r\n\r\n",
                    )
                    .await;
                let chunk = vec![b'A'; 512];
                let header = format!("{:x}\r\n", chunk.len());
                for _ in 0..10 {
                    if sock.write_all(header.as_bytes()).await.is_err() {
                        break;
                    }
                    if sock.write_all(&chunk).await.is_err() {
                        break;
                    }
                    if sock.write_all(b"\r\n").await.is_err() {
                        break;
                    }
                }
                let _ = sock.write_all(b"0\r\n\r\n").await;
            }
        });

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{addr}/feed.ics"))
            .send()
            .await
            .expect("send");
        let err = read_response_capped(resp, 1024)
            .await
            .expect_err("must reject oversized chunked body");
        assert!(matches!(err, ReadCappedError::TooLarge));
        let _ = server.await;
    }
}
