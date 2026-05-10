// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::collections::HashSet;
use std::net::Ipv4Addr;

use tokio::sync::mpsc;

use libgvls::{Account, Vni, Vtep};

#[derive(Debug)]
pub struct GetAccountByIdReq {
    pub account_id: i32,
    pub rep_tx: mpsc::Sender<GetAccountByIdRep>,
}

#[derive(Debug)]
pub struct GetAccountByIdRep {
    pub account: Option<Account>,
}

#[derive(Debug)]
pub struct AuthAccountReq {
    pub mail_addr: String,
    pub password: String,
    pub rep_tx: mpsc::Sender<AuthAccountRep>,
}

#[derive(Debug)]
pub struct AuthAccountRep {
    pub account_id: Result<i32, String>,
}

#[derive(Debug)]
pub struct AuthRrReq {
    pub name: String,
    pub password: String,
    pub addr: Ipv4Addr,
    pub rep_tx: mpsc::Sender<AuthRrRep>,
}

#[derive(Debug)]
pub struct AuthRrRep {
    pub rr_name: Result<String, String>,
    pub vteps: Vec<Vtep>,
    pub vnis: Vec<Vni>,
}

#[derive(Debug, Clone)]
pub struct OpRep {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug)]
pub struct ListVtepsReq {
    pub requester_id: i32,
    pub rep_tx: mpsc::Sender<ListVtepsRep>,
}

#[derive(Debug)]
pub struct ListVtepsRep {
    pub requester: Option<Account>,
    pub accounts: Vec<Account>,
    pub vteps: Vec<Vtep>,
    pub assignable_vnis: Vec<Vni>,
}

#[derive(Debug)]
pub struct CreateVtepReq {
    pub requester_id: i32,
    pub owner_account_id: Option<i32>,
    pub description: String,
    pub password: String,
    pub rep_tx: mpsc::Sender<OpRep>,
}

#[derive(Debug)]
pub struct UpdateVtepVnisReq {
    pub requester_id: i32,
    pub vtep_name: String,
    pub description: String,
    pub vnis: HashSet<i32>,
    pub rep_tx: mpsc::Sender<OpRep>,
}

#[derive(Debug)]
pub struct DeleteVtepReq {
    pub requester_id: i32,
    pub vtep_name: String,
    pub rep_tx: mpsc::Sender<OpRep>,
}

#[derive(Debug)]
pub struct ListVnisReq {
    pub requester_id: i32,
    pub rep_tx: mpsc::Sender<ListVnisRep>,
}

#[derive(Debug)]
pub struct ListVnisRep {
    pub requester: Option<Account>,
    pub accounts: Vec<Account>,
    pub vnis: Vec<Vni>,
    pub assignable_vteps: Vec<Vtep>,
}

#[derive(Debug)]
pub struct CreateVniReq {
    pub requester_id: i32,
    pub owner_account_id: Option<i32>,
    pub description: String,
    pub rep_tx: mpsc::Sender<OpRep>,
}

#[derive(Debug)]
pub struct UpdateVniVtepsReq {
    pub requester_id: i32,
    pub vni: i32,
    pub description: String,
    pub vteps: HashSet<String>,
    pub rep_tx: mpsc::Sender<OpRep>,
}

#[derive(Debug)]
pub struct DeleteVniReq {
    pub requester_id: i32,
    pub vni: i32,
    pub rep_tx: mpsc::Sender<OpRep>,
}

#[derive(Debug)]
pub struct ListAccountsReq {
    pub requester_id: i32,
    pub rep_tx: mpsc::Sender<ListAccountsRep>,
}

#[derive(Debug)]
pub struct ListAccountsRep {
    pub requester: Option<Account>,
    pub accounts: Vec<Account>,
}

#[derive(Debug)]
pub struct CreateAccountReq {
    pub requester_id: i32,
    pub mail_addr: String,
    pub password: String,
    pub perm_code: i32,
    pub rep_tx: mpsc::Sender<OpRep>,
}

#[derive(Debug)]
pub struct DeleteAccountReq {
    pub requester_id: i32,
    pub account_id: i32,
    pub rep_tx: mpsc::Sender<OpRep>,
}

#[derive(Debug)]
pub struct UpdateAccountPermReq {
    pub requester_id: i32,
    pub account_id: i32,
    pub perm_code: i32,
    pub rep_tx: mpsc::Sender<OpRep>,
}

#[derive(Debug)]
pub struct ChangePasswordReq {
    pub account_id: i32,
    pub current_password: String,
    pub new_password: String,
    pub rep_tx: mpsc::Sender<OpRep>,
}

#[derive(Debug)]
pub enum UiLchMsg {
    GetAccountById(GetAccountByIdReq),
    AuthAccount(AuthAccountReq),
    AuthRr(AuthRrReq),
    ListVteps(ListVtepsReq),
    CreateVtep(CreateVtepReq),
    UpdateVtepVnis(UpdateVtepVnisReq),
    DeleteVtep(DeleteVtepReq),
    ListVnis(ListVnisReq),
    CreateVni(CreateVniReq),
    UpdateVniVteps(UpdateVniVtepsReq),
    DeleteVni(DeleteVniReq),
    ListAccounts(ListAccountsReq),
    CreateAccount(CreateAccountReq),
    DeleteAccount(DeleteAccountReq),
    UpdateAccountPerm(UpdateAccountPermReq),
    ChangePassword(ChangePasswordReq),
}

#[derive(Debug, Clone)]
pub enum RrLchMsg {
    VtepAdded(Vtep),
    VtepDeleted(Vtep),
    VniAdded(Vni),
    VniDeleted(Vni),
    VtepVniModified { vtep: Vtep, vni: Vni },
}
