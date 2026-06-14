//! Server-Sent Events stream of new-message notifications, for foreground
//! desktop notifications when web push isn't available (e.g. Brave).
//!
//! EventSource can't send an Authorization header, so the short-lived access
//! token is passed as a `?token=` query parameter and validated here.

use axum::{
    extract::{Query, State},
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::{error::AppError, state::AppState};

#[derive(Deserialize)]
pub struct EventsQuery {
    token: String,
}

pub async fn events_stream(
    State(state): State<AppState>,
    Query(query): Query<EventsQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    let user_id = state
        .jwt_key
        .validate(&query.token)
        .map_err(|_| AppError::Unauthorized)?;

    let rx = state.events.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(move |result| {
        let event = result.ok()?;
        if event.user_id != user_id {
            return None;
        }
        Some(Ok(Event::default().event("message").data(event.payload)))
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(30))))
}
