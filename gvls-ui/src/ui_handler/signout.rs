// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use async_session::{MemoryStore, SessionStore};
use axum::extract::{FromRef, State};
use axum::response::{IntoResponse, Redirect};
use axum_extra::TypedHeader;
use axum_extra::headers::Cookie;

use crate::ui_handler::{AppState, COOKIE_NAME};

pub async fn signout_get(
    State(state): State<AppState>,
    cookies: Option<TypedHeader<Cookie>>,
) -> Result<impl IntoResponse, String> {
    let store = MemoryStore::from_ref(&state);
    let Some(TypedHeader(cookies)) = cookies else {
        return Ok(Redirect::to("/"));
    };
    let cookie = match cookies.get(COOKIE_NAME) {
        Some(cookie) => cookie,
        None => return Ok(Redirect::to("/")),
    };
    let session = match store.load_session(cookie.to_string()).await {
        Ok(Some(s)) => s,
        _ => return Ok(Redirect::to("/")),
    };
    if let Err(e) = store.destroy_session(session).await {
        return Err(format!("failed to destroy session: {e}"));
    }
    Ok(Redirect::to("/"))
}
