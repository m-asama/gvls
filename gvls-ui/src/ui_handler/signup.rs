// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use askama::Template;
use axum::extract::State;
use axum::response::{Html, IntoResponse};

use crate::ui_handler::{AppSession, AppState, Authority};

#[derive(Template)]
#[template(path = "signup.html")]
struct SignupTemplate {
    auth: Authority,
}

pub async fn signup_get(
    State(state): State<AppState>,
    session: Option<AppSession>,
) -> Result<impl IntoResponse, String> {
    let auth = super::auth(&state, &session).await;
    let html = SignupTemplate { auth }
        .render()
        .map_err(|e| format!("Template render error: {e}"))?;
    Ok(Html(html))
}
