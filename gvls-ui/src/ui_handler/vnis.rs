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
    CreateVniReq, DeleteVniReq, ListVnisRep, ListVnisReq, OpRep, UiLchMsg, UpdateVniVtepsReq,
};

#[derive(Clone)]
struct OwnerOption {
    id: i32,
    mail_addr: String,
}

#[derive(Clone)]
struct VtepChoice {
    name: String,
    checked: bool,
}

#[derive(Clone)]
struct VniRow {
    vni: i32,
    owner_mail_addr: String,
    description: String,
    owned_choices: Vec<VtepChoice>,
    other_choices: Vec<VtepChoice>,
}

#[derive(Template)]
#[template(path = "vnis.html")]
struct VnisTemplate {
    auth: Authority,
    message: String,
    is_admin: bool,
    owner_options: Vec<OwnerOption>,
    rows: Vec<VniRow>,
}

#[derive(Debug, Deserialize)]
pub struct VniInput {
    action: String,
    owner_account_id: Option<i32>,
    description: Option<String>,
    vni_value: Option<i32>,
    vteps: Option<Vec<String>>,
}

async fn load_vnis(state: &AppState, account_id: i32) -> Result<ListVnisRep, String> {
    let (rep_tx, mut rep_rx) = mpsc::channel(1);
    state
        .tx_lch
        .send(UiLchMsg::ListVnis(ListVnisReq {
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

fn build_rows(rep: &ListVnisRep) -> Vec<VniRow> {
    let is_admin = rep
        .requester
        .as_ref()
        .map(|account| account.perm == libgvls::Permission::Admin)
        .unwrap_or(false);
    rep.vnis
        .iter()
        .map(|vni| {
            let mut vteps = vni.vteps.iter().cloned().collect::<Vec<_>>();
            vteps.sort();
            let choices = rep
                .assignable_vteps
                .iter()
                .map(|vtep| VtepChoice {
                    name: vtep.name.clone(),
                    checked: vni.vteps.contains(&vtep.name),
                })
                .collect::<Vec<_>>();
            let (mut owned_choices, mut other_choices) = if is_admin {
                choices.into_iter().partition::<Vec<_>, _>(|choice| {
                    rep.assignable_vteps
                        .iter()
                        .find(|vtep| vtep.name == choice.name)
                        .map(|vtep| vtep.account_id == vni.account_id)
                        .unwrap_or(false)
                })
            } else {
                (choices, Vec::new())
            };
            owned_choices.sort_by(|a, b| a.name.cmp(&b.name));
            other_choices.sort_by(|a, b| a.name.cmp(&b.name));
            VniRow {
                vni: vni.vni,
                owner_mail_addr: rep
                    .accounts
                    .iter()
                    .find(|account| account.id == vni.account_id)
                    .map(|account| account.mail_addr.clone())
                    .unwrap_or_else(|| "-".to_string()),
                description: vni.description.clone(),
                owned_choices,
                other_choices,
            }
        })
        .collect()
}

fn build_owners(rep: &ListVnisRep) -> Vec<OwnerOption> {
    rep.accounts
        .iter()
        .map(|account| OwnerOption {
            id: account.id,
            mail_addr: account.mail_addr.clone(),
        })
        .collect()
}

async fn render_page(
    state: &AppState,
    account_id: i32,
    auth: Authority,
    message: String,
) -> Result<Response, String> {
    let rep = load_vnis(state, account_id).await?;
    let template = VnisTemplate {
        auth,
        message,
        is_admin: auth == Authority::Admin,
        owner_options: build_owners(&rep),
        rows: build_rows(&rep),
    };
    let html = template
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

pub async fn vnis_get(
    State(state): State<AppState>,
    session: AppSession,
) -> Result<Response, String> {
    let account = current_account(&state, &Some(session)).await?;
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

pub async fn vnis_post(
    State(state): State<AppState>,
    session: AppSession,
    Form(input): Form<VniInput>,
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
                .send(UiLchMsg::CreateVni(CreateVniReq {
                    requester_id: account.id,
                    owner_account_id: input.owner_account_id,
                    description: input.description.unwrap_or_default(),
                    rep_tx,
                }))
                .await
                .map_err(|_| "channel send error".to_string())?;
        }
        "update" => {
            state
                .tx_lch
                .send(UiLchMsg::UpdateVniVteps(UpdateVniVtepsReq {
                    requester_id: account.id,
                    vni: input.vni_value.unwrap_or_default(),
                    description: input.description.unwrap_or_default(),
                    vteps: input.vteps.unwrap_or_default().into_iter().collect(),
                    rep_tx,
                }))
                .await
                .map_err(|_| "channel send error".to_string())?;
        }
        "delete" => {
            state
                .tx_lch
                .send(UiLchMsg::DeleteVni(DeleteVniReq {
                    requester_id: account.id,
                    vni: input.vni_value.unwrap_or_default(),
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
        return Ok(Redirect::to("/vnis").into_response());
    }
    render_page(&state, account.id, auth, rep.message).await
}
