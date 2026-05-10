// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use askama::Template;
use axum::extract::State;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::Form;
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::ui_handler::{AppSession, AppState, Authority, current_account};
use crate::{ChangePasswordReq, OpRep, UiLchMsg};

#[derive(Template)]
#[template(path = "setting.html")]
struct SettingTemplate {
    auth: Authority,
    mail_addr: String,
    message: String,
}

#[derive(Debug, Deserialize)]
pub struct SettingInput {
    current_password: String,
    new_password: String,
    new_password_confirm: String,
}

async fn render_page(
    auth: Authority,
    mail_addr: String,
    message: String,
) -> Result<Response, String> {
    let html = SettingTemplate {
        auth,
        mail_addr,
        message,
    }
    .render()
    .map_err(|e| format!("Template render error: {e}"))?;
    Ok(Html(html).into_response())
}

async fn wait_op(mut rep_rx: mpsc::Receiver<OpRep>) -> Result<OpRep, String> {
    rep_rx
        .recv()
        .await
        .ok_or_else(|| "channel recv error".to_string())
}

pub async fn setting_get(
    State(state): State<AppState>,
    session: AppSession,
) -> Result<Response, String> {
    let account = current_account(
        &state,
        &Some(AppSession {
            account_id: session.account_id,
        }),
    )
    .await?;
    let Some(account) = account else {
        return Ok(Redirect::to("/signin").into_response());
    };
    render_page(
        super::authority_from_account(Some(&account)),
        account.mail_addr,
        String::new(),
    )
    .await
}

pub async fn setting_post(
    State(state): State<AppState>,
    session: AppSession,
    Form(input): Form<SettingInput>,
) -> Result<Response, String> {
    let account = current_account(
        &state,
        &Some(AppSession {
            account_id: session.account_id,
        }),
    )
    .await?;
    let Some(account) = account else {
        return Ok(Redirect::to("/signin").into_response());
    };
    let auth = super::authority_from_account(Some(&account));
    if input.new_password != input.new_password_confirm {
        return render_page(
            auth,
            account.mail_addr,
            "New passwords do not match".to_string(),
        )
        .await;
    }
    let (rep_tx, rep_rx) = mpsc::channel(1);
    state
        .tx_lch
        .send(UiLchMsg::ChangePassword(ChangePasswordReq {
            account_id: account.id,
            current_password: input.current_password,
            new_password: input.new_password,
            rep_tx,
        }))
        .await
        .map_err(|_| "channel send error".to_string())?;
    let rep = wait_op(rep_rx).await?;
    if rep.ok {
        return Ok(Redirect::to("/setting").into_response());
    }
    render_page(auth, account.mail_addr, rep.message).await
}
