// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use askama::Template;
use async_session::{MemoryStore, Session, SessionStore};
use axum::extract::{FromRef, Query, State};
use axum::http::HeaderMap;
use axum::http::header::SET_COOKIE;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::Form;
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::ui_handler::{AppSession, AppState, Authority, COOKIE_NAME};
use crate::{AuthAccountReq, UiLchMsg};

#[derive(Template)]
#[template(path = "signin.html")]
struct SigninTemplate {
    auth: super::Authority,
    error: String,
}

#[derive(Debug, Deserialize)]
pub struct SigninInput {
    pub mail_addr: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct SigninQuery {
    error: Option<String>,
}

pub async fn signin_get(
    State(state): State<AppState>,
    session: Option<AppSession>,
    Query(query): Query<SigninQuery>,
) -> Result<impl IntoResponse, String> {
    let auth = super::auth(&state, &session).await;
    let html = SigninTemplate {
        auth,
        error: query.error.unwrap_or_default(),
    }
    .render()
    .map_err(|e| format!("Template render error: {e}"))?;
    Ok(Html(html))
}

pub async fn signin_post(
    State(state): State<AppState>,
    _session: Option<AppSession>,
    Form(input): Form<SigninInput>,
) -> Result<Response, String> {
    let store = MemoryStore::from_ref(&state);
    let (rep_tx, mut rep_rx) = mpsc::channel(1);
    let req = AuthAccountReq {
        mail_addr: input.mail_addr,
        password: input.password,
        rep_tx,
    };
    state
        .tx_lch
        .send(UiLchMsg::AuthAccount(req))
        .await
        .map_err(|_| "channel send error".to_string())?;
    if let Some(rep) = rep_rx.recv().await {
        if let Ok(account_id) = rep.account_id {
            let app_session = AppSession { account_id };
            let mut new_session = Session::new();
            new_session
                .insert("app_session", &app_session)
                .map_err(|e| format!("session insert error: {e}"))?;
            let cookie = store
                .store_session(new_session)
                .await
                .map_err(|e| format!("store session error: {e}"))?
                .ok_or_else(|| "store session error".to_string())?;
            let cookie = format!("{COOKIE_NAME}={cookie}; SameSite=Lax; HttpOnly; Path=/");
            let cookie = cookie
                .parse()
                .map_err(|_| "failed to parse cookie".to_string())?;
            let mut headers = HeaderMap::new();
            headers.insert(SET_COOKIE, cookie);
            return Ok((headers, Redirect::to("/")).into_response());
        }
    }
    Ok(Redirect::to("/signin?error=Authentication%20failed").into_response())
}
