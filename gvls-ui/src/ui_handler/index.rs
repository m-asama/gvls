// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use askama::Template;
use axum::extract::State;
use axum::response::{Html, IntoResponse};

use crate::ui_handler::{AppSession, AppState, Authority};

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    auth: Authority,
}

pub async fn index_get(
    State(state): State<AppState>,
    session: Option<AppSession>,
) -> Result<impl IntoResponse, String> {
    let auth = super::auth(&state, &session).await;
    let html = IndexTemplate { auth }
        .render()
        .map_err(|e| format!("Template render error: {e}"))?;
    Ok(Html(html))
}
