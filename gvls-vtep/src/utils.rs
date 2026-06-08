// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::net::Ipv6Addr;

use libgvls::{BgpOps, exec, is_bridge, link_exists, sysctl};

fn brname(vni: u32) -> String {
    format!("gvls-br{vni}")
}

fn vniname(vni: u32) -> String {
    format!("gvls-vni{vni}")
}

pub async fn add_vni(vni: u32, ifname: &str, local: &Option<Ipv6Addr>) -> Result<(), String> {
    if !link_exists(ifname).await {
        return Err(format!("Link {ifname} not exists"));
    }

    let brname = brname(vni);
    let vniname = vniname(vni);

    // br setup
    if !link_exists(&brname).await {
        exec(vec!["ip", "link", "add", &brname, "type", "bridge"]).await;
        exec(vec!["ip", "link", "set", &brname, "addrgenmode", "none"]).await;
    }
    if !is_bridge(&brname).await {
        return Err(format!("Link {brname} is not bridge"));
    }
    sysctl(&format!("net.ipv4.conf.{brname}.forwarding=0")).await;
    sysctl(&format!("net.ipv6.conf.{brname}.forwarding=0")).await;
    sysctl(&format!("net.ipv6.conf.{brname}.disable_ipv6=1")).await;
    exec(vec!["ip", "link", "set", &brname, "up"]).await;

    // vni setup
    if link_exists(&vniname).await {
        exec(vec!["ip", "link", "delete", &vniname]).await;
    }
    if let Err(e) = update_vni(vni, &None, local).await {
        return Err(format!("Init {vni} error: {e}"));
    }

    // brport setup
    sysctl(&format!("net.ipv6.conf.{ifname}.disable_ipv6=1")).await;
    exec(vec!["ip", "link", "set", ifname, "up"]).await;
    exec(vec!["ip", "link", "set", ifname, "master", &brname]).await;

    Ok(())
}

pub async fn del_vni(vni: u32) -> Result<(), String> {
    let brname = brname(vni);
    let vniname = vniname(vni);

    if link_exists(&vniname).await {
        exec(vec!["ip", "link", "delete", &vniname]).await;
    }

    if link_exists(&brname).await {
        exec(vec!["ip", "link", "delete", &brname]).await;
    }

    Ok(())
}

pub async fn update_vni(
    vni: u32,
    loc_current: &Option<Ipv6Addr>,
    loc_next: &Option<Ipv6Addr>,
) -> Result<(), String> {
    println!("Update VNI {vni}: local {:?}->{:?}", loc_current, loc_next);
    let brname = brname(vni);
    let vniname = vniname(vni);
    if loc_next.is_none() || loc_next != loc_current {
        exec(vec!["ip", "link", "delete", &vniname]).await;
    }
    if let Some(local) = loc_next
        && loc_next != loc_current
    {
        exec(
            [
                vec!["ip", "link", "add", &vniname, "type", "vxlan"],
                vec!["local", &format!("{local}"), "dstport", "4789"],
                vec!["id", &format!("{vni}"), "nolearning"],
            ]
            .concat(),
        )
        .await;
        exec(
            [
                vec!["ip", "link", "set", &vniname],
                vec!["master", &brname, "addrgenmode", "none"],
            ]
            .concat(),
        )
        .await;
        exec(
            [
                vec!["ip", "link", "set", &vniname, "type", "bridge_slave"],
                vec!["neigh_suppress", "on", "learning", "off"],
            ]
            .concat(),
        )
        .await;
        sysctl(&format!("net.ipv6.conf.{vniname}.disable_ipv6=1")).await;
        exec(vec!["ip", "link", "set", &vniname, "up"]).await;
    }
    Ok(())
}

pub async fn update_bgp(
    loc_current: &Option<Ipv6Addr>,
    loc_next: &Option<Ipv6Addr>,
    rem_current: &Option<Ipv6Addr>,
    rem_next: &Option<Ipv6Addr>,
    bgp_ops: &BgpOps,
    asnum: u32,
    name: &str,
    pass: &str,
) -> Result<(), String> {
    if loc_current != loc_next || rem_current != rem_next {
        println!(
            "Update BGP neighbor {name}: local {:?}->{:?}, remote {:?}->{:?}",
            loc_current, loc_next, rem_current, rem_next
        );
    }
    match (rem_current, rem_next) {
        (None, None) => {}
        (Some(rem_current), None) => {
            if loc_current.is_some() {
                bgp_ops.del_neighbor(asnum, rem_current).await;
            }
        }
        (None, Some(rem_next)) => {
            bgp_ops
                .add_neighbor(asnum, rem_next, loc_next, name, pass, false)
                .await;
        }
        (Some(rem_current), Some(rem_next)) => match (loc_current, loc_next) {
            (None, None) => {}
            (Some(_loc_current), None) => {
                bgp_ops.del_neighbor(asnum, rem_current).await;
            }
            (None, Some(loc_next)) => {
                bgp_ops
                    .add_neighbor(asnum, rem_next, &Some(loc_next.clone()), name, pass, false)
                    .await;
            }
            (Some(loc_current), Some(loc_next)) => {
                if rem_current != rem_next {
                    bgp_ops.del_neighbor(asnum, rem_current).await;
                    bgp_ops
                        .add_neighbor(asnum, rem_next, &Some(loc_next.clone()), name, pass, false)
                        .await;
                } else if loc_current != loc_next {
                    bgp_ops.upd_neighbor_us(asnum, rem_next, loc_next).await;
                }
            }
        },
    }
    Ok(())
}

pub async fn init_neighs() -> Result<(), String> {
    exec(
        [
            vec!["ipset", "create", "gvls-neighs", "hash:net"],
            vec!["family", "inet6", "hashsize", "1024", "maxelem", "65536"],
        ]
        .concat(),
    )
    .await;
    Ok(())
}

pub async fn add_neigh(neigh: &Ipv6Addr) -> Result<(), String> {
    exec(vec!["ipset", "add", "gvls-neighs", &format!("{neigh}")]).await;
    Ok(())
}

pub async fn del_neigh(neigh: &Ipv6Addr) -> Result<(), String> {
    exec(vec!["ipset", "del", "gvls-neighs", &format!("{neigh}")]).await;
    Ok(())
}
