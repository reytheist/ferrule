//! Lightweight self-hostable HTTP + SSE companion server for Pigment Staff Sync.
//!
//! Implements the `/v1` wire contract consumed by `mail-core::sync::staff`:
//! - Simple Bearer token auth (invite/session tokens, zero commercial SaaS bloat)
//! - SQLite persistence with zero external database dependencies
//! - Live Server-Sent Events (`GET /v1/events`) broadcasting `StaffSyncEvent` updates

pub mod db;

use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post, put};
use axum::{Json, Router};
use mail_core::calendar::{ActionItemView, CalendarEventView, MeetingDocView};
use mail_core::ids::{
    ActionItemId, CalendarEventId, ContactId, PinnedLinkId, SharedTemplateId, SharedThreadId,
    StaffMemberId, UnixMs,
};
use mail_core::staff::{
    HandoffStatus, PinnedLinkView, SharedCommentView, SharedDraftView, SharedTemplateView,
    SharedThreadView, ShowContactView, StaffMemberView,
};
use mail_core::{StaffSyncEvent, StaffSyncSnapshot};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, oneshot, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};

use crate::db::{DbError, SyncDb};

const DEFAULT_MAX_SSE_CONNECTIONS: usize = 64;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub db_path: Option<PathBuf>,
    pub tokens: Vec<String>,
    pub heartbeat_interval: Duration,
    pub max_sse_connections: usize,
}

impl ServerConfig {
    pub fn with_tokens(tokens: Vec<String>, db_path: Option<PathBuf>) -> Self {
        Self {
            db_path,
            tokens,
            heartbeat_interval: Duration::from_secs(15),
            max_sse_connections: DEFAULT_MAX_SSE_CONNECTIONS,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("at least one staff sync token must be configured")]
    NoTokens,
    #[error("database error: {0}")]
    Db(#[from] DbError),
    #[error("bind/serve error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct AppState {
    db: SyncDb,
    tokens: Vec<String>,
    events: broadcast::Sender<StaffSyncEvent>,
    heartbeat_interval: Duration,
    sse_semaphore: Arc<Semaphore>,
}

impl AppState {
    pub fn new(config: ServerConfig) -> Result<Arc<Self>, ServerError> {
        let tokens: Vec<String> = config
            .tokens
            .into_iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        if tokens.is_empty() {
            return Err(ServerError::NoTokens);
        }
        let db = SyncDb::open(config.db_path.as_deref())?;
        let (events, _) = broadcast::channel(256);
        let max_sse = config.max_sse_connections.max(1);
        Ok(Arc::new(Self {
            db,
            tokens,
            events,
            heartbeat_interval: config.heartbeat_interval,
            sse_semaphore: Arc::new(Semaphore::new(max_sse)),
        }))
    }

    fn is_authorized(&self, candidate: &str) -> bool {
        self.tokens
            .iter()
            .any(|expected| constant_time_eq(expected.as_bytes(), candidate.as_bytes()))
    }

    fn broadcast(&self, event: StaffSyncEvent) {
        let _ = self.events.send(event);
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn now_unix_ms() -> UnixMs {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    UnixMs(millis)
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_rev: Option<u32>,
}

struct ApiError(DbError);

impl From<DbError> for ApiError {
    fn from(value: DbError) -> Self {
        Self(value)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self.0 {
            DbError::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                Json(ErrorBody {
                    error: msg,
                    current_rev: None,
                }),
            )
                .into_response(),
            DbError::NotFound(entity) => (
                StatusCode::NOT_FOUND,
                Json(ErrorBody {
                    error: format!("{entity} not found"),
                    current_rev: None,
                }),
            )
                .into_response(),
            DbError::Conflict { current_rev } => (
                StatusCode::CONFLICT,
                Json(ErrorBody {
                    error: "stale revision".to_string(),
                    current_rev: Some(current_rev),
                }),
            )
                .into_response(),
            other => {
                eprintln!("staff-sync-server internal error: {other}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorBody {
                        error: "internal server error".to_string(),
                        current_rev: None,
                    }),
                )
                    .into_response()
            }
        }
    }
}

async fn require_bearer_auth(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|raw| raw.strip_prefix("Bearer "))
        .map(str::trim);

    match token {
        Some(t) if state.is_authorized(t) => next.run(request).await,
        _ => (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            Json(ErrorBody {
                error: "invalid or missing bearer token".to_string(),
                current_rev: None,
            }),
        )
            .into_response(),
    }
}

pub fn build_router(state: Arc<AppState>) -> Router {
    let v1 = Router::new()
        .route("/snapshot", get(get_snapshot))
        .route("/events", get(get_events))
        .route("/members/{id}", put(put_member))
        .route("/shared-threads", post(post_shared_thread))
        .route("/shared-threads/{id}/handoff", patch(patch_handoff))
        .route("/shared-threads/{id}/comments", post(post_comment))
        .route("/shared-threads/{id}/draft", put(put_draft))
        .route(
            "/shared-templates/{id}",
            put(put_template).delete(delete_template),
        )
        .route("/contacts/{id}", put(put_contact).delete(delete_contact))
        .route(
            "/pinned-links/{id}",
            put(put_pinned_link).delete(delete_pinned_link),
        )
        .route(
            "/calendar/events/{id}",
            put(put_calendar_event).delete(delete_calendar_event),
        )
        .route("/calendar/events/{id}/doc", put(put_meeting_doc))
        .route(
            "/action-items/{id}",
            put(put_action_item).delete(delete_action_item),
        )
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_bearer_auth,
        ));

    Router::new()
        .route("/health", get(get_health))
        .nest("/v1", v1)
        .with_state(state)
}

pub struct RunningServer {
    pub addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

impl RunningServer {
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

pub async fn spawn_server(
    bind_addr: SocketAddr,
    config: ServerConfig,
) -> Result<RunningServer, ServerError> {
    let state = AppState::new(config)?;
    let router = build_router(state);
    let listener = TcpListener::bind(bind_addr).await?;
    let addr = listener.local_addr()?;
    let (tx, rx) = oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await;
    });
    Ok(RunningServer {
        addr,
        shutdown: Some(tx),
        handle: Some(handle),
    })
}

// ---------------------------------------------------------------- handlers

async fn get_health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

#[derive(Deserialize)]
struct SnapshotQuery {
    since: Option<i64>,
}

async fn get_snapshot(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SnapshotQuery>,
) -> Result<Json<StaffSyncSnapshot>, ApiError> {
    let since = query.since.map(UnixMs);
    let snapshot = state.db.snapshot(since, now_unix_ms())?;
    Ok(Json(snapshot))
}

struct PermitStream<S> {
    inner: Pin<Box<S>>,
    _permit: OwnedSemaphorePermit,
}

impl<S: Stream> Stream for PermitStream<S> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

async fn get_events(State(state): State<Arc<AppState>>) -> Response {
    let Ok(permit) = state.sse_semaphore.clone().try_acquire_owned() else {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ErrorBody {
                error: "too many concurrent SSE connections".to_string(),
                current_rev: None,
            }),
        )
            .into_response();
    };
    let rx = state.events.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|item| match item {
        Ok(ev) => Event::default().event("update").json_data(ev).ok().map(Ok::<Event, Infallible>),
        Err(_) => Event::default()
            .event("update")
            .json_data(StaffSyncEvent::SnapshotInvalidated { since: None })
            .ok()
            .map(Ok::<Event, Infallible>),
    });
    let guarded = PermitStream {
        inner: Box::pin(stream),
        _permit: permit,
    };
    Sse::new(guarded)
        .keep_alive(
            KeepAlive::new()
                .interval(state.heartbeat_interval)
                .text("keep-alive"),
        )
        .into_response()
}

async fn put_member(
    State(state): State<Arc<AppState>>,
    Path(id): Path<StaffMemberId>,
    Json(member): Json<StaffMemberView>,
) -> Result<StatusCode, ApiError> {
    if member.id != id {
        return Err(ApiError(DbError::BadRequest(
            "member id in request body must match path id".to_string(),
        )));
    }
    state.db.upsert_member(&member, now_unix_ms())?;
    state.broadcast(StaffSyncEvent::DirectoryUpdated);
    Ok(StatusCode::NO_CONTENT)
}

async fn post_shared_thread(
    State(state): State<Arc<AppState>>,
    Json(thread): Json<SharedThreadView>,
) -> Result<StatusCode, ApiError> {
    let id = thread.id;
    state.db.upsert_shared_thread(&thread, now_unix_ms())?;
    state.broadcast(StaffSyncEvent::ThreadUpdated {
        shared_thread_id: id,
    });
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HandoffPayload {
    assignee_handle: Option<String>,
    status: HandoffStatus,
}

async fn patch_handoff(
    State(state): State<Arc<AppState>>,
    Path(id): Path<SharedThreadId>,
    Json(payload): Json<HandoffPayload>,
) -> Result<StatusCode, ApiError> {
    state
        .db
        .update_handoff(id, payload.assignee_handle, payload.status, now_unix_ms())?;
    state.broadcast(StaffSyncEvent::ThreadUpdated {
        shared_thread_id: id,
    });
    Ok(StatusCode::NO_CONTENT)
}

async fn post_comment(
    State(state): State<Arc<AppState>>,
    Path(id): Path<SharedThreadId>,
    Json(comment): Json<SharedCommentView>,
) -> Result<StatusCode, ApiError> {
    state.db.insert_comment(id, &comment, now_unix_ms())?;
    state.broadcast(StaffSyncEvent::CommentAdded {
        shared_thread_id: id,
    });
    Ok(StatusCode::NO_CONTENT)
}

async fn put_draft(
    State(state): State<Arc<AppState>>,
    Path(id): Path<SharedThreadId>,
    Json(draft): Json<SharedDraftView>,
) -> Result<StatusCode, ApiError> {
    state.db.upsert_draft(id, &draft, now_unix_ms())?;
    state.broadcast(StaffSyncEvent::DraftUpdated {
        shared_thread_id: id,
    });
    Ok(StatusCode::NO_CONTENT)
}

async fn put_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<SharedTemplateId>,
    Json(template): Json<SharedTemplateView>,
) -> Result<StatusCode, ApiError> {
    state.db.upsert_template(id, &template, now_unix_ms())?;
    state.broadcast(StaffSyncEvent::ResourcesUpdated);
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_template(
    State(state): State<Arc<AppState>>,
    Path(id): Path<SharedTemplateId>,
) -> Result<StatusCode, ApiError> {
    state.db.delete_template(id, now_unix_ms())?;
    state.broadcast(StaffSyncEvent::ResourcesUpdated);
    Ok(StatusCode::NO_CONTENT)
}

async fn put_contact(
    State(state): State<Arc<AppState>>,
    Path(id): Path<ContactId>,
    Json(contact): Json<ShowContactView>,
) -> Result<StatusCode, ApiError> {
    state.db.upsert_contact(id, &contact, now_unix_ms())?;
    state.broadcast(StaffSyncEvent::ResourcesUpdated);
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_contact(
    State(state): State<Arc<AppState>>,
    Path(id): Path<ContactId>,
) -> Result<StatusCode, ApiError> {
    state.db.delete_contact(id, now_unix_ms())?;
    state.broadcast(StaffSyncEvent::ResourcesUpdated);
    Ok(StatusCode::NO_CONTENT)
}

async fn put_pinned_link(
    State(state): State<Arc<AppState>>,
    Path(id): Path<PinnedLinkId>,
    Json(link): Json<PinnedLinkView>,
) -> Result<StatusCode, ApiError> {
    state.db.upsert_pinned_link(id, &link, now_unix_ms())?;
    state.broadcast(StaffSyncEvent::ResourcesUpdated);
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_pinned_link(
    State(state): State<Arc<AppState>>,
    Path(id): Path<PinnedLinkId>,
) -> Result<StatusCode, ApiError> {
    state.db.delete_pinned_link(id, now_unix_ms())?;
    state.broadcast(StaffSyncEvent::ResourcesUpdated);
    Ok(StatusCode::NO_CONTENT)
}

async fn put_calendar_event(
    State(state): State<Arc<AppState>>,
    Path(id): Path<CalendarEventId>,
    Json(event): Json<CalendarEventView>,
) -> Result<StatusCode, ApiError> {
    state.db.upsert_calendar_event(id, &event, now_unix_ms())?;
    state.broadcast(StaffSyncEvent::CalendarUpdated { event_id: id });
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_calendar_event(
    State(state): State<Arc<AppState>>,
    Path(id): Path<CalendarEventId>,
) -> Result<StatusCode, ApiError> {
    state.db.delete_calendar_event(id, now_unix_ms())?;
    state.broadcast(StaffSyncEvent::CalendarUpdated { event_id: id });
    Ok(StatusCode::NO_CONTENT)
}

async fn put_meeting_doc(
    State(state): State<Arc<AppState>>,
    Path(id): Path<CalendarEventId>,
    Json(doc): Json<MeetingDocView>,
) -> Result<StatusCode, ApiError> {
    state.db.upsert_meeting_doc(id, &doc, now_unix_ms())?;
    state.broadcast(StaffSyncEvent::MeetingDocUpdated { event_id: id });
    Ok(StatusCode::NO_CONTENT)
}

async fn put_action_item(
    State(state): State<Arc<AppState>>,
    Path(id): Path<ActionItemId>,
    Json(item): Json<ActionItemView>,
) -> Result<StatusCode, ApiError> {
    state.db.upsert_action_item(id, &item, now_unix_ms())?;
    state.broadcast(StaffSyncEvent::ActionItemUpdated { action_item_id: id });
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_action_item(
    State(state): State<Arc<AppState>>,
    Path(id): Path<ActionItemId>,
) -> Result<StatusCode, ApiError> {
    state.db.delete_action_item(id, now_unix_ms())?;
    state.broadcast(StaffSyncEvent::ActionItemUpdated { action_item_id: id });
    Ok(StatusCode::NO_CONTENT)
}
