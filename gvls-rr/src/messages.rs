// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::collections::{HashMap, HashSet};
use std::net::Ipv6Addr;

use tokio::sync::mpsc;

use libgvls::{Vni, Vtep};

#[derive(Debug)]
pub struct RrRegisteredMsg {
    pub vteps: HashMap<String, Vtep>,
    pub vnis: HashMap<i32, Vni>,
}

#[derive(Debug, Clone)]
pub struct LocAddrChangedMsg {
    pub loc_addr: Option<Ipv6Addr>,
}

#[derive(Debug)]
pub struct AuthVtepReq {
    pub name: String,
    pub password: String,
    pub rem_addr: Ipv6Addr,
    pub rep_tx: mpsc::Sender<AuthVtepRep>,
}

#[derive(Debug)]
pub struct AuthVtepRep {
    pub vtep_name: Result<String, String>,
    pub bgp_pass: String,
    pub neighs: HashSet<Ipv6Addr>,
}

#[derive(Debug)]
pub struct UpdateNeighsMsg {
    pub neighs: HashSet<Ipv6Addr>,
}

#[derive(Debug)]
pub struct VtepRegisteredMsg {
    pub name: String,
    pub rem_addr: Ipv6Addr,
}

#[derive(Debug)]
pub struct VtepExitMsg {
    pub name: String,
    pub rem_addr: Ipv6Addr,
}

#[derive(Debug)]
pub struct AddVtepMsg {
    pub vtep: Vtep,
}

#[derive(Debug)]
pub struct DelVtepMsg {
    pub vtep: Vtep,
}

#[derive(Debug)]
pub struct AddVniMsg {
    pub vni: Vni,
}

#[derive(Debug)]
pub struct DelVniMsg {
    pub vni: Vni,
}

#[derive(Debug)]
pub struct ModVtepVniMsg {
    pub vtep: Vtep,
    pub vni: Vni,
}

#[derive(Debug)]
pub enum RrLchMsg {
    LocAddrChanged(LocAddrChangedMsg),
    RrRegistered(RrRegisteredMsg),
    AddVtep(AddVtepMsg),
    DelVtep(DelVtepMsg),
    AddVni(AddVniMsg),
    DelVni(DelVniMsg),
    ModVtepVni(ModVtepVniMsg),
    AuthVtep(AuthVtepReq),
    VtepRegistered(VtepRegisteredMsg),
    VtepExit(VtepExitMsg),
}

#[derive(Debug)]
pub enum UiLchMsg {}

#[derive(Debug)]
pub enum VtepLchMsg {
    UpdateNeighs(UpdateNeighsMsg),
}
