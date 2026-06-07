// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::collections::HashSet;
use std::net::Ipv6Addr;

use tokio::process::Command;

const ROUTE_MAP_PREFIX: &str = "gvlm-rm";

#[derive(Debug)]
pub struct BgpOpsFrr {}

impl BgpOpsFrr {
    pub async fn add_neighbor(
        &self,
        asnum: u32,
        rem_addr: &Ipv6Addr,
        loc_addr: &Option<Ipv6Addr>,
        name: &str,
        pass: &str,
        rr: bool,
    ) {
        let loc_addr = match loc_addr {
            Some(loc_addr) => loc_addr,
            None => return,
        };
        let rmname = format!("{ROUTE_MAP_PREFIX}-{name}");
        if let Err(e) = Command::new("vtysh")
            .arg("-c")
            .arg("configure terminal")
            .arg("-c")
            .arg(format!("router bgp {asnum}"))
            .arg("-c")
            .arg(format!("neighbor {rem_addr} remote-as internal"))
            .arg("-c")
            .arg(format!("neighbor {rem_addr} description {name}"))
            .arg("-c")
            .arg(format!("neighbor {rem_addr} password {pass}"))
            .arg("-c")
            .arg(format!("neighbor {rem_addr} update-source {loc_addr}"))
            .output()
            .await
        {
            println!("Add neighbor error: {e}");
        }
        if rr {
            if let Err(e) = Command::new("vtysh")
                .arg("-c")
                .arg("configure terminal")
                .arg("-c")
                .arg(format!("router bgp {asnum}"))
                .arg("-c")
                .arg("address-family l2vpn evpn")
                .arg("-c")
                .arg(format!("neighbor {rem_addr} route-reflector-client"))
                .arg("-c")
                .arg(format!("neighbor {rem_addr} route-map {rmname} in"))
                .arg("-c")
                .arg(format!("neighbor {rem_addr} route-map {rmname} out"))
                .output()
                .await
            {
                println!("Set neighbor route-map error: {e}");
            }
        }
        if let Err(e) = Command::new("vtysh")
            .arg("-c")
            .arg("configure terminal")
            .arg("-c")
            .arg(format!("router bgp {asnum}"))
            .arg("-c")
            .arg("address-family l2vpn evpn")
            .arg("-c")
            .arg(format!("neighbor {rem_addr} activate"))
            .output()
            .await
        {
            println!("Set neighbor activate error: {e}");
        }
    }

    pub async fn del_neighbor(&self, asnum: u32, rem_addr: &Ipv6Addr) {
        if let Err(e) = Command::new("vtysh")
            .arg("-c")
            .arg("configure terminal")
            .arg("-c")
            .arg(format!("router bgp {asnum}"))
            .arg("-c")
            .arg(format!("no neighbor {rem_addr}"))
            .output()
            .await
        {
            println!("Delete neighbor error: {e}");
        }
    }

    pub async fn upd_neighbor_us(&self, asnum: u32, rem_addr: &Ipv6Addr, loc_addr: &Ipv6Addr) {
        if let Err(e) = Command::new("vtysh")
            .arg("-c")
            .arg("configure terminal")
            .arg("-c")
            .arg(format!("router bgp {asnum}"))
            .arg("-c")
            .arg(format!("neighbor {rem_addr} update-source {loc_addr}"))
            .output()
            .await
        {
            println!("Update neighbor update-source error: {e}");
        }
    }

    pub async fn upd_neighbor_pass(&self, asnum: u32, rem_addr: &Ipv6Addr, pass: &String) {
        if let Err(e) = Command::new("vtysh")
            .arg("-c")
            .arg("configure terminal")
            .arg("-c")
            .arg(format!("router bgp {asnum}"))
            .arg("-c")
            .arg(format!("neighbor {rem_addr} password {pass}"))
            .output()
            .await
        {
            println!("Update neighbor password error: {e}");
        }
    }

    pub async fn rep_route_map(&self, name: &String, vni_ids: &HashSet<i32>) {
        self.del_route_map(name).await;
        let rmname = format!("{ROUTE_MAP_PREFIX}-{name}");
        let mut i = 1;
        for vni_id in vni_ids {
            if let Err(e) = Command::new("vtysh")
                .arg("-c")
                .arg("configure terminal")
                .arg("-c")
                .arg(format!("route-map {rmname} permit {i}"))
                .arg("-c")
                .arg("match evpn route-type macip")
                .arg("-c")
                .arg(format!("match evpn vni {vni_id}"))
                .output()
                .await
            {
                println!("Replace route map error: {e}");
            }
            i += 1;
            if let Err(e) = Command::new("vtysh")
                .arg("-c")
                .arg("configure terminal")
                .arg("-c")
                .arg(format!("route-map {rmname} permit {i}"))
                .arg("-c")
                .arg("match evpn route-type multicast")
                .arg("-c")
                .arg(format!("match evpn vni {vni_id}"))
                .output()
                .await
            {
                println!("Replace route map error: {e}");
            }
            i += 1;
        }
        if let Err(e) = Command::new("vtysh")
            .arg("-c")
            .arg("configure terminal")
            .arg("-c")
            .arg(format!("route-map {rmname} deny {i}"))
            .output()
            .await
        {
            println!("Replace route map error: {e}");
        }
    }

    pub async fn del_route_map(&self, name: &String) {
        let rmname = format!("{ROUTE_MAP_PREFIX}-{name}");
        if let Err(e) = Command::new("vtysh")
            .arg("-c")
            .arg("configure terminal")
            .arg("-c")
            .arg(format!("no route-map {rmname}"))
            .output()
            .await
        {
            println!("Delete route map error: {e}");
        }
    }

    pub async fn init(&self, asnum: u32) {
        if let Err(e) = Command::new("vtysh")
            .arg("-c")
            .arg("configure terminal")
            .arg("-c")
            .arg("ipv6 nht resolve-via-default")
            .arg("-c")
            .arg(format!("router bgp {asnum}"))
            .arg("-c")
            .arg("address-family l2vpn evpn")
            .arg("-c")
            .arg("advertise-all-vni")
            .output()
            .await
        {
            println!("Init BGP error: {e}");
        }
    }

    pub async fn ready(&self) -> Result<(), String> {
        if let Ok(status) = Command::new("vtysh")
            .arg("-c")
            .arg("show version")
            .status()
            .await
        {
            if status.success() {
                return Ok(());
            }
        }
        Err(format!("Not ready"))
    }
}
