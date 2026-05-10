// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use askama::Template;
use axum::extract::State;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::Form;
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::ui_handler::{AppSession, AppState, Authority, current_account};
use crate::{
    CreateAccountReq, DeleteAccountReq, ListAccountsRep, ListAccountsReq, OpRep, UiLchMsg,
    UpdateAccountPermReq,
};

#[derive(Clone)]
struct AccountRow {
    id: i32,
    mail_addr: String,
    perm_code: i32,
    perm_label: String,
    vni_count: usize,
    vtep_count: usize,
    can_change_perm: bool,
}

#[derive(Template)]
#[template(path = "accounts.html")]
struct AccountsTemplate {
    auth: Authority,
    message: String,
    rows: Vec<AccountRow>,
}

#[derive(Debug, Deserialize)]
pub struct AccountsInput {
    action: String,
    account_id: Option<i32>,
    mail_addr: Option<String>,
    password: Option<String>,
    perm: Option<i32>,
}

async fn load_accounts(state: &AppState, account_id: i32) -> Result<ListAccountsRep, String> {
    let (rep_tx, mut rep_rx) = mpsc::channel(1);
    state
        .tx_lch
        .send(UiLchMsg::ListAccounts(ListAccountsReq {
            requester_id: account_id,
            rep_tx,
        }))
        .await
        .map_err(|_| "channel send error".to_string())?;
    rep_rx
        .recv()
        .await
        .ok_or_else(|| "channel recv error".to_string())
}

fn build_rows(rep: &ListAccountsRep) -> Vec<AccountRow> {
    rep.accounts
        .iter()
        .map(|account| AccountRow {
            id: account.id,
            mail_addr: account.mail_addr.clone(),
            perm_code: crate::Context::permission_code(account.perm),
            perm_label: match account.perm {
                libgvls::Permission::Admin => "Admin".to_string(),
                libgvls::Permission::Free => "Free".to_string(),
                libgvls::Permission::Pro => "Pro".to_string(),
            },
            vni_count: account.vnis.len(),
            vtep_count: account.vteps.len(),
            can_change_perm: account.perm != libgvls::Permission::Admin,
        })
        .collect()
}

async fn render_page(
    state: &AppState,
    account_id: i32,
    auth: Authority,
    message: String,
) -> Result<Response, String> {
    if auth != Authority::Admin {
        return Ok(Redirect::to("/").into_response());
    }
    let rep = load_accounts(state, account_id).await?;
    let html = AccountsTemplate {
        auth,
        message,
        rows: build_rows(&rep),
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

pub async fn accounts_get(
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
        &state,
        account.id,
        super::authority_from_account(Some(&account)),
        String::new(),
    )
    .await
}

pub async fn accounts_post(
    State(state): State<AppState>,
    session: AppSession,
    Form(input): Form<AccountsInput>,
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
    let (rep_tx, rep_rx) = mpsc::channel(1);
    match input.action.as_str() {
        "create" => {
            state
                .tx_lch
                .send(UiLchMsg::CreateAccount(CreateAccountReq {
                    requester_id: account.id,
                    mail_addr: input.mail_addr.unwrap_or_default(),
                    password: input.password.unwrap_or_default(),
                    perm_code: input.perm.unwrap_or(2),
                    rep_tx,
                }))
                .await
                .map_err(|_| "channel send error".to_string())?;
        }
        "delete" => {
            state
                .tx_lch
                .send(UiLchMsg::DeleteAccount(DeleteAccountReq {
                    requester_id: account.id,
                    account_id: input.account_id.unwrap_or_default(),
                    rep_tx,
                }))
                .await
                .map_err(|_| "channel send error".to_string())?;
        }
        "save" => {
            state
                .tx_lch
                .send(UiLchMsg::UpdateAccountPerm(UpdateAccountPermReq {
                    requester_id: account.id,
                    account_id: input.account_id.unwrap_or_default(),
                    perm_code: input.perm.unwrap_or_default(),
                    rep_tx,
                }))
                .await
                .map_err(|_| "channel send error".to_string())?;
        }
        _ => {
            return render_page(&state, account.id, auth, "Unknown action".to_string()).await;
        }
    }
    let rep = wait_op(rep_rx).await?;
    if rep.ok {
        return Ok(Redirect::to("/accounts").into_response());
    }
    render_page(&state, account.id, auth, rep.message).await
}
