// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::collections::HashSet;
use std::net::Ipv6Addr;

use futures_util::stream::StreamExt;
use netlink_packet_core::NetlinkPayload;
use netlink_packet_route::RouteNetlinkMessage::{DelAddress, NewAddress};
use rtnetlink::{MulticastGroup, new_multicast_connection};
use tokio::sync::mpsc;

use libgvls::{get_ipv6_addrs, parse_ipv6_addr};

use crate::{LocAddrChangedMsg, VtepLchMsg};

pub struct RtnlHandler {
    src_ifname: String,
    src_ifindex: u32,
    tx_lch: mpsc::Sender<VtepLchMsg>,
    //
    loc_addr_active: Option<Ipv6Addr>,
    loc_addr_candidates: HashSet<Ipv6Addr>,
}

impl RtnlHandler {
    pub fn new(src_ifname: String, src_ifindex: u32, tx_lch: mpsc::Sender<VtepLchMsg>) -> Self {
        Self {
            src_ifname,
            src_ifindex,
            tx_lch,
            //
            loc_addr_active: None,
            loc_addr_candidates: HashSet::<Ipv6Addr>::new(),
        }
    }
    async fn init_loc_addrs(&mut self) {
        let addrs = match get_ipv6_addrs(&self.src_ifname).await {
            Ok(addrs) => addrs,
            Err(e) => {
                println!("Get IPv6 addresses error: {e}");
                return;
            }
        };
        let mut loc_addr_active: Option<Ipv6Addr> = None;
        let mut loc_addr_candidates = HashSet::<Ipv6Addr>::new();
        if addrs.len() > 0 {
            loc_addr_active = Some(addrs[0].clone());
            for addr in addrs {
                loc_addr_candidates.insert(addr);
            }
        }
        self.loc_addr_active = loc_addr_active;
        self.loc_addr_candidates = loc_addr_candidates;
    }
    async fn send_loc_addr_changed(&mut self, loc_addr: Option<Ipv6Addr>) {
        let msg = VtepLchMsg::LocAddrChanged(LocAddrChangedMsg { loc_addr: loc_addr });
        let _ = self.tx_lch.send(msg).await;
    }
    pub async fn run(&mut self) {
        let loc_addr_orig = self.loc_addr_active.clone();
        self.init_loc_addrs().await;
        let (conn, mut msgs) = match new_multicast_connection(&[MulticastGroup::Ipv6Ifaddr]) {
            Ok((conn, _, msgs)) => (conn, msgs),
            Err(e) => {
                println!("new multicast connection error: {e}");
                return;
            }
        };
        tokio::spawn(conn);
        if self.loc_addr_active != loc_addr_orig {
            println!(
                "Initial local address selected on {}: {:?}",
                self.src_ifname, self.loc_addr_active
            );
            self.send_loc_addr_changed(self.loc_addr_active.clone())
                .await;
        }
        while let Some((msg, _)) = msgs.next().await {
            match msg.payload {
                NetlinkPayload::InnerMessage(NewAddress(msg)) => {
                    if msg.header.index != self.src_ifindex {
                        continue;
                    }
                    if let Some(addr) = parse_ipv6_addr(msg) {
                        println!("Local address appeared on {}: {}", self.src_ifname, addr);
                        self.loc_addr_candidates.insert(addr);
                        if self.loc_addr_active.is_none() {
                            self.loc_addr_active = Some(addr.clone());
                            self.send_loc_addr_changed(Some(addr)).await;
                        }
                    }
                }
                NetlinkPayload::InnerMessage(DelAddress(msg)) => {
                    if msg.header.index != self.src_ifindex {
                        continue;
                    }
                    if let Some(addr) = parse_ipv6_addr(msg) {
                        println!(
                            "Local address disappeared from {}: {}",
                            self.src_ifname, addr
                        );
                        self.loc_addr_candidates.remove(&addr);
                        let loc_addr_old = self.loc_addr_active.clone();
                        let mut loc_addr_new = self.loc_addr_active.clone();
                        let deleted_active_addr = if let Some(active_addr) = &self.loc_addr_active
                            && addr == *active_addr
                        {
                            true
                        } else {
                            false
                        };
                        if deleted_active_addr {
                            loc_addr_new = None;
                            for tmp in &self.loc_addr_candidates {
                                loc_addr_new = Some(tmp.clone());
                                break;
                            }
                        }
                        self.loc_addr_active = loc_addr_new.clone();
                        if loc_addr_new != loc_addr_old {
                            self.send_loc_addr_changed(loc_addr_new).await;
                        }
                    }
                }
                _ => {}
            }
        }
        println!("rtnl handler exit");
    }
}
