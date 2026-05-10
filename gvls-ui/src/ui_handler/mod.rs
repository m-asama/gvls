// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::convert::Infallible;
use std::net::IpAddr;

use async_session::{MemoryStore, SessionStore};
use axum::extract::{FromRef, FromRequestParts, OptionalFromRequestParts};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::{RequestPartsExt, Router};
use axum_extra::TypedHeader;
use axum_extra::headers::Cookie;
use http::request::Parts;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tower_http::services::ServeDir;

use libgvls::{Account, Permission};

use crate::{GetAccountByIdReq, UiLchMsg};

mod accounts;
mod index;
mod setting;
mod signin;
mod signout;
mod signup;
mod vnis;
mod vteps;

use accounts::*;
use index::*;
use setting::*;
use signin::*;
use signout::*;
use signup::*;
use vnis::*;
use vteps::*;

pub static COOKIE_NAME: &str = "SESSION";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authority {
    Anonymous,
    Admin,
    Free,
    Pro,
}

pub struct AuthRedirect;

impl IntoResponse for AuthRedirect {
    fn into_response(self) -> Response {
        Redirect::temporary("/signin").into_response()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppSession {
    pub account_id: i32,
}

impl<S> FromRequestParts<S> for AppSession
where
    MemoryStore: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AuthRedirect;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let store = MemoryStore::from_ref(state);
        let cookies = parts
            .extract::<TypedHeader<Cookie>>()
            .await
            .map_err(|_| AuthRedirect)?;
        let session_cookie = cookies.get(COOKIE_NAME).ok_or(AuthRedirect)?;
        let session = store
            .load_session(session_cookie.to_string())
            .await
            .unwrap()
            .ok_or(AuthRedirect)?;
        let app_session = session
            .get::<AppSession>("app_session")
            .ok_or(AuthRedirect)?;
        Ok(app_session)
    }
}

impl<S> OptionalFromRequestParts<S> for AppSession
where
    MemoryStore: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        match <AppSession as FromRequestParts<S>>::from_request_parts(parts, state).await {
            Ok(res) => Ok(Some(res)),
            Err(AuthRedirect) => Ok(None),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub store: MemoryStore,
    pub tx_lch: mpsc::Sender<UiLchMsg>,
}

impl FromRef<AppState> for MemoryStore {
    fn from_ref(state: &AppState) -> Self {
        state.store.clone()
    }
}

pub fn authority_from_account(account: Option<&Account>) -> Authority {
    match account.map(|account| account.perm) {
        Some(Permission::Admin) => Authority::Admin,
        Some(Permission::Free) => Authority::Free,
        Some(Permission::Pro) => Authority::Pro,
        None => Authority::Anonymous,
    }
}

pub async fn current_account(
    state: &AppState,
    session: &Option<AppSession>,
) -> Result<Option<Account>, String> {
    let Some(session) = session else {
        return Ok(None);
    };
    let (rep_tx, mut rep_rx) = mpsc::channel(1);
    let req = GetAccountByIdReq {
        account_id: session.account_id,
        rep_tx,
    };
    state
        .tx_lch
        .send(UiLchMsg::GetAccountById(req))
        .await
        .map_err(|_| "channel send error".to_string())?;
    Ok(rep_rx.recv().await.and_then(|rep| rep.account))
}

pub async fn auth(state: &AppState, session: &Option<AppSession>) -> Authority {
    match current_account(state, session).await {
        Ok(account) => authority_from_account(account.as_ref()),
        Err(_) => Authority::Anonymous,
    }
}

pub struct UiHandler {
    addr: IpAddr,
    port: u16,
    tx_lch: mpsc::Sender<UiLchMsg>,
}

impl UiHandler {
    pub fn new(addr: IpAddr, port: u16, tx_lch: mpsc::Sender<UiLchMsg>) -> Self {
        Self { addr, port, tx_lch }
    }

    pub async fn run(&mut self) {
        let store = MemoryStore::new();
        let app_state = AppState {
            store,
            tx_lch: self.tx_lch.clone(),
        };
        let app = Router::new()
            .route("/", get(index_get))
            .route("/signup", get(signup_get))
            .route("/signin", get(signin_get).post(signin_post))
            .route("/signout", get(signout_get))
            .route("/vteps", get(vteps_get).post(vteps_post))
            .route("/vnis", get(vnis_get).post(vnis_post))
            .route("/setting", get(setting_get).post(setting_post))
            .route("/accounts", get(accounts_get).post(accounts_post))
            .nest_service("/img", ServeDir::new("public/img"))
            .nest_service("/css", ServeDir::new("public/css"))
            .nest_service("/js", ServeDir::new("public/js"))
            .with_state(app_state);
        let listener = tokio::net::TcpListener::bind((self.addr, self.port))
            .await
            .unwrap();
        let _ = axum::serve(listener, app).await;
    }
}
