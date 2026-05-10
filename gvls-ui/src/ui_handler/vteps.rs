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
    CreateVtepReq, DeleteVtepReq, ListVtepsRep, ListVtepsReq, OpRep, UiLchMsg, UpdateVtepVnisReq,
};

#[derive(Clone)]
struct OwnerOption {
    id: i32,
    mail_addr: String,
}

#[derive(Clone)]
struct VniChoice {
    vni: i32,
    checked: bool,
}

#[derive(Clone)]
struct VtepRow {
    name: String,
    owner_mail_addr: String,
    description: String,
    owned_choices: Vec<VniChoice>,
    other_choices: Vec<VniChoice>,
}

#[derive(Template)]
#[template(path = "vteps.html")]
struct VtepsTemplate {
    auth: Authority,
    message: String,
    is_admin: bool,
    owner_options: Vec<OwnerOption>,
    rows: Vec<VtepRow>,
}

#[derive(Debug, Deserialize)]
pub struct VtepInput {
    action: String,
    owner_account_id: Option<i32>,
    description: Option<String>,
    password: Option<String>,
    vtep_name: Option<String>,
    vnis: Option<Vec<i32>>,
}

async fn load_vteps(state: &AppState, account_id: i32) -> Result<ListVtepsRep, String> {
    let (rep_tx, mut rep_rx) = mpsc::channel(1);
    state
        .tx_lch
        .send(UiLchMsg::ListVteps(ListVtepsReq {
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

fn build_rows(rep: &ListVtepsRep) -> Vec<VtepRow> {
    let is_admin = rep
        .requester
        .as_ref()
        .map(|account| account.perm == libgvls::Permission::Admin)
        .unwrap_or(false);
    rep.vteps
        .iter()
        .map(|vtep| {
            let choices = rep
                .assignable_vnis
                .iter()
                .map(|vni| VniChoice {
                    vni: vni.vni,
                    checked: vtep.vnis.contains(&vni.vni),
                })
                .collect::<Vec<_>>();
            let (mut owned_choices, mut other_choices) = if is_admin {
                choices.into_iter().partition::<Vec<_>, _>(|choice| {
                    rep.assignable_vnis
                        .iter()
                        .find(|vni| vni.vni == choice.vni)
                        .map(|vni| vni.account_id == vtep.account_id)
                        .unwrap_or(false)
                })
            } else {
                (choices, Vec::new())
            };
            owned_choices.sort_by(|a, b| a.vni.cmp(&b.vni));
            other_choices.sort_by(|a, b| a.vni.cmp(&b.vni));
            let mut vnis = vtep.vnis.iter().copied().collect::<Vec<_>>();
            vnis.sort_unstable();
            VtepRow {
                name: vtep.name.clone(),
                owner_mail_addr: rep
                    .accounts
                    .iter()
                    .find(|account| account.id == vtep.account_id)
                    .map(|account| account.mail_addr.clone())
                    .unwrap_or_else(|| "-".to_string()),
                description: vtep.description.clone(),
                owned_choices,
                other_choices,
            }
        })
        .collect()
}

fn build_owners(rep: &ListVtepsRep) -> Vec<OwnerOption> {
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
    let rep = load_vteps(state, account_id).await?;
    let template = VtepsTemplate {
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

pub async fn vteps_get(
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

pub async fn vteps_post(
    State(state): State<AppState>,
    session: AppSession,
    Form(input): Form<VtepInput>,
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
                .send(UiLchMsg::CreateVtep(CreateVtepReq {
                    requester_id: account.id,
                    owner_account_id: input.owner_account_id,
                    description: input.description.unwrap_or_default(),
                    password: input.password.unwrap_or_default(),
                    rep_tx,
                }))
                .await
                .map_err(|_| "channel send error".to_string())?;
        }
        "update" => {
            state
                .tx_lch
                .send(UiLchMsg::UpdateVtepVnis(UpdateVtepVnisReq {
                    requester_id: account.id,
                    vtep_name: input.vtep_name.unwrap_or_default(),
                    description: input.description.unwrap_or_default(),
                    vnis: input.vnis.unwrap_or_default().into_iter().collect(),
                    rep_tx,
                }))
                .await
                .map_err(|_| "channel send error".to_string())?;
        }
        "delete" => {
            state
                .tx_lch
                .send(UiLchMsg::DeleteVtep(DeleteVtepReq {
                    requester_id: account.id,
                    vtep_name: input.vtep_name.unwrap_or_default(),
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
        return Ok(Redirect::to("/vteps").into_response());
    }
    render_page(&state, account.id, auth, rep.message).await
}
