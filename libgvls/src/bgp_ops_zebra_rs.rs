// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::collections::HashSet;
use std::net::Ipv6Addr;
use std::time::Duration;

use tokio::process::Command;
use tokio::time::sleep;

const ROUTE_MAP_PREFIX: &str = "gvlm-rm";

#[derive(Debug)]
pub struct BgpOpsZebraRs {}

impl BgpOpsZebraRs {
    async fn exec(&self, cmd: &str) {
        if let Err(e) = Command::new("vtyctl")
            .arg("apply")
            .arg("-c")
            .arg(cmd)
            .output()
            .await
        {
            eprintln!("vtyctl apply failed: {e}");
        }
    }

    pub async fn add_neighbor(
        &self,
        asnum: u32,
        rem_addr: &Ipv6Addr,
        loc_addr: &Option<Ipv6Addr>,
        name: &str,
        pass: &str,
        rr: bool,
    ) {
        let Some(loc_addr) = loc_addr else {
            return;
        };
        let rmname = format!("{ROUTE_MAP_PREFIX}-{name}");
        self.exec(&format!(
            "set router bgp neighbor {rem_addr} remote-as {asnum}
set router bgp neighbor {rem_addr} description {name}
set router bgp neighbor {rem_addr} password {pass}
set router bgp neighbor {rem_addr} update-source {loc_addr}
set router bgp neighbor {rem_addr} afi-safi evpn enabled true"
        ))
        .await;
        if rr {
            self.exec(&format!(
                "set router bgp neighbor {rem_addr} route-reflector client true
set router bgp neighbor {rem_addr} policy in {rmname}
set router bgp neighbor {rem_addr} policy out {rmname}"
            ))
            .await;
        }
    }

    pub async fn del_neighbor(&self, _asnum: u32, rem_addr: &Ipv6Addr) {
        self.exec(&format!("delete router bgp neighbor {rem_addr}"))
            .await;
    }

    pub async fn upd_neighbor_us(&self, _asnum: u32, rem_addr: &Ipv6Addr, loc_addr: &Ipv6Addr) {
        self.exec(&format!(
            "set router bgp neighbor {rem_addr} update-source {loc_addr}"
        ))
        .await;
    }

    pub async fn upd_neighbor_pass(&self, _asnum: u32, rem_addr: &Ipv6Addr, pass: &String) {
        self.exec(&format!(
            "set router bgp neighbor {rem_addr} password {pass}"
        ))
        .await;
    }

    pub async fn rep_route_map(&self, name: &String, vni_ids: &HashSet<i32>) {
        self.del_route_map(name).await;
        let rmname = format!("{ROUTE_MAP_PREFIX}-{name}");
        let mut i = 1;
        for vni_id in vni_ids {
            self.exec(&format!(
                "set policy {rmname} entry {i} match evpn route-type macip
set policy {rmname} entry {i} match evpn vni {vni_id}
set policy {rmname} entry {i} action permit"
            ))
            .await;
            i += 1;
            self.exec(&format!(
                "set policy {rmname} entry {i} match evpn route-type multicast
set policy {rmname} entry {i} match evpn vni {vni_id}
set policy {rmname} entry {i} action permit
"
            ))
            .await;
            i += 1;
        }
    }

    pub async fn del_route_map(&self, name: &String) {
        let rmname = format!("{ROUTE_MAP_PREFIX}-{name}");
        self.exec(&format!("delete policy {rmname}")).await;
    }

    pub async fn init(&self, asnum: u32) {
        self.exec(&format!(
            "set router bgp global as {asnum}
set router bgp afi-safi evpn advertise-all-vni true"
        ))
        .await;
    }

    pub async fn wait(&self) -> Result<(), String> {
        for _ in 0..60 {
            if let Ok(status) = Command::new("vtyctl")
                .arg("show")
                .arg("version")
                .status()
                .await
            {
                if status.success() {
                    return Ok(());
                }
            }
            sleep(Duration::from_secs(1)).await;
        }
        Err(format!("Waiting zebra-rs timed out"))
    }
}
