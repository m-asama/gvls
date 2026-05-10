// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::collections::HashSet;
use std::fmt::{Display, Error, Formatter};
use std::net::Ipv6Addr;

use crate::{BgpOpsFrr, BgpOpsZebraRs};

#[derive(Debug)]
pub enum BgpOps {
    Frr(BgpOpsFrr),
    ZebraRs(BgpOpsZebraRs),
}

impl Display for BgpOps {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match self {
            BgpOps::Frr(_) => write!(f, "frr"),
            BgpOps::ZebraRs(_) => write!(f, "zebra-rs"),
        }
    }
}

impl BgpOps {
    pub async fn add_neighbor(
        &self,
        asnum: u32,
        rem_addr: &Ipv6Addr,
        loc_addr: &Option<Ipv6Addr>,
        name: &str,
        pass: &str,
        route_map: bool,
    ) {
        match self {
            BgpOps::Frr(bgp_ops) => {
                bgp_ops
                    .add_neighbor(asnum, rem_addr, loc_addr, name, pass, route_map)
                    .await
            }
            BgpOps::ZebraRs(bgp_ops) => {
                bgp_ops
                    .add_neighbor(asnum, rem_addr, loc_addr, name, pass, route_map)
                    .await
            }
        }
    }

    pub async fn del_neighbor(&self, asnum: u32, rem_addr: &Ipv6Addr) {
        match self {
            BgpOps::Frr(bgp_ops) => bgp_ops.del_neighbor(asnum, rem_addr).await,
            BgpOps::ZebraRs(bgp_ops) => bgp_ops.del_neighbor(asnum, rem_addr).await,
        }
    }

    pub async fn upd_neighbor_us(&self, asnum: u32, rem_addr: &Ipv6Addr, loc_addr: &Ipv6Addr) {
        match self {
            BgpOps::Frr(bgp_ops) => bgp_ops.upd_neighbor_us(asnum, rem_addr, loc_addr).await,
            BgpOps::ZebraRs(bgp_ops) => bgp_ops.upd_neighbor_us(asnum, rem_addr, loc_addr).await,
        }
    }

    pub async fn upd_neighbor_pass(&self, asnum: u32, rem_addr: &Ipv6Addr, pass: &String) {
        match self {
            BgpOps::Frr(bgp_ops) => bgp_ops.upd_neighbor_pass(asnum, rem_addr, pass).await,
            BgpOps::ZebraRs(bgp_ops) => bgp_ops.upd_neighbor_pass(asnum, rem_addr, pass).await,
        }
    }

    pub async fn rep_route_map(&self, name: &String, vni_ids: &HashSet<i32>) {
        match self {
            BgpOps::Frr(bgp_ops) => bgp_ops.rep_route_map(name, vni_ids).await,
            BgpOps::ZebraRs(bgp_ops) => bgp_ops.rep_route_map(name, vni_ids).await,
        }
    }

    pub async fn del_route_map(&self, name: &String) {
        match self {
            BgpOps::Frr(bgp_ops) => bgp_ops.del_route_map(name).await,
            BgpOps::ZebraRs(bgp_ops) => bgp_ops.del_route_map(name).await,
        }
    }

    pub async fn init(&self, asnum: u32) {
        match self {
            BgpOps::Frr(bgp_ops) => bgp_ops.init(asnum).await,
            BgpOps::ZebraRs(bgp_ops) => bgp_ops.init(asnum).await,
        }
    }
}
