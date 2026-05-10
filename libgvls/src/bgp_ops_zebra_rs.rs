// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::collections::HashSet;
use std::net::Ipv6Addr;

#[derive(Debug)]
pub struct BgpOpsZebraRs {}

impl BgpOpsZebraRs {
    pub async fn add_neighbor(
        &self,
        _asnum: u32,
        _rem_addr: &Ipv6Addr,
        _loc_addr: &Option<Ipv6Addr>,
        _name: &str,
        _pass: &str,
        _route_map: bool,
    ) {
    }

    pub async fn del_neighbor(&self, _asnum: u32, _rem_addr: &Ipv6Addr) {}

    pub async fn upd_neighbor_us(&self, _asnum: u32, _rem_addr: &Ipv6Addr, _loc_addr: &Ipv6Addr) {}

    pub async fn upd_neighbor_pass(&self, _asnum: u32, _rem_addr: &Ipv6Addr, _pass: &String) {}

    pub async fn rep_route_map(&self, _name: &String, _vni_ids: &HashSet<i32>) {}

    pub async fn del_route_map(&self, _name: &String) {}

    pub async fn init(&self, _asnum: u32) {}
}
