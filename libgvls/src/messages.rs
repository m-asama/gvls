// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::net::Ipv6Addr;

use remoc::rch;

use crate::{Vni, Vtep};

//
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct HelloMsg {}

//
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RegisterRrReqMsg {
    pub name: String,
    pub password: String,
    pub rep_tx: rch::mpsc::Sender<RegisterRrRepMsg>,
}

//
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RegisterRrRepMsg {
    pub vtep_rx: rch::mpsc::Receiver<Vtep>,
    pub vni_rx: rch::mpsc::Receiver<Vni>,
}

//
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RegisterVtepReqMsg {
    pub name: String,
    pub password: String,
    pub rep_tx: rch::mpsc::Sender<RegisterVtepRepMsg>,
}

//
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RegisterVtepRepMsg {
    pub bgp_pass: String,
    pub neigh_rx: rch::mpsc::Receiver<Ipv6Addr>,
}

//
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct VtepAddedMsg {
    pub vtep: Vtep,
}

//
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct VtepDeletedMsg {
    pub vtep: Vtep,
}

//
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct VniAddedMsg {
    pub vni: Vni,
}

//
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct VniDeletedMsg {
    pub vni: Vni,
}

//
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct VtepVniModifiedMsg {
    pub vtep: Vtep,
    pub vni: Vni,
}

//
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct NeighsUpdatedMsg {
    pub neigh_rx: rch::mpsc::Receiver<Ipv6Addr>,
}

//
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum UiRchMsg {
    Hello(HelloMsg),
    RegisterRrReq(RegisterRrReqMsg),
}

//
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum RrRchMsg {
    Hello(HelloMsg),
    RegisterVtepReq(RegisterVtepReqMsg),
    VtepAdded(VtepAddedMsg),
    VtepDeleted(VtepDeletedMsg),
    VniAdded(VniAddedMsg),
    VniDeleted(VniDeletedMsg),
    VtepVniModified(VtepVniModifiedMsg),
}

//
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum VtepRchMsg {
    Hello(HelloMsg),
    NeighsUpdated(NeighsUpdatedMsg),
}
