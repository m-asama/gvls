// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::collections::HashSet;
use std::net::Ipv6Addr;

#[derive(Debug, Clone)]
pub struct VtepRegisteredMsg {
    pub rr_index: usize,
    pub bgp_pass: String,
    pub neighs: HashSet<Ipv6Addr>,
}

#[derive(Debug, Clone)]
pub struct LocAddrChangedMsg {
    pub loc_addr: Option<Ipv6Addr>,
}

#[derive(Debug, Clone)]
pub struct RemAddrChangedMsg {
    pub rr_index: usize,
    pub rem_addr: Option<Ipv6Addr>,
}

#[derive(Debug, Clone)]
pub struct UpdateNeighsMsg {
    pub rr_index: usize,
    pub neighs: HashSet<Ipv6Addr>,
}

#[derive(Debug, Clone)]
pub enum VtepLchMsg {
    VtepRegistered(VtepRegisteredMsg),
    LocAddrChanged(LocAddrChangedMsg),
    RemAddrChanged(RemAddrChangedMsg),
    UpdateNeighs(UpdateNeighsMsg),
}

#[derive(Debug, Clone)]
pub enum RrLchMsg {
    LocAddrChanged(LocAddrChangedMsg),
}
