// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::net::Ipv6Addr;

use tokio::process::Command;

use libgvls::{BgpOps, is_bridge, is_vxlan, link_exists};

pub async fn init_vni(vni: u32, ifname: &str, local: &Option<Ipv6Addr>) -> Result<(), String> {
    if !link_exists(ifname).await {
        return Err(format!("Link {ifname} not exists"));
    }
    let brname = format!("br{vni}");
    let vniname = format!("vni{vni}");
    let vnistr = format!("{vni}");
    let localstr = if let Some(local) = local {
        format!("{local}")
    } else {
        let l1 = format!("{:02x}", vni >> 16);
        let l2 = format!("{:x}", vni & 0xffffu32);
        format!("::ffff:7f{l1}:{l2}")
    };
    if !link_exists(&brname).await {
        if let Err(e) = Command::new("ip")
            .arg("link")
            .arg("add")
            .arg(&brname)
            .arg("type")
            .arg("bridge")
            .output()
            .await
        {
            println!("ip link add {brname} error: {e}");
        }
        if let Err(e) = Command::new("ip")
            .arg("link")
            .arg("set")
            .arg(&brname)
            .arg("addrgenmode")
            .arg("none")
            .output()
            .await
        {
            println!("ip link set {brname} addrgenmode none error: {e}");
        }
    }
    if !is_bridge(&brname).await {
        return Err(format!("Link {brname} is not bridge"));
    }
    if let Err(e) = Command::new("ip")
        .arg("link")
        .arg("set")
        .arg(&brname)
        .arg("up")
        .output()
        .await
    {
        println!("ip link set {brname} up error: {e}");
    }
    if let Err(e) = Command::new("sysctl")
        .arg("-w")
        .arg(&format!("net.ipv4.conf.{brname}.forwarding=0"))
        .output()
        .await
    {
        println!("sysctl {brname} IPv4 forward disable error: {e}");
    }
    if let Err(e) = Command::new("sysctl")
        .arg("-w")
        .arg(&format!("net.ipv6.conf.{brname}.forwarding=0"))
        .output()
        .await
    {
        println!("sysctl {brname} IPv6 forward disable error: {e}");
    }
    if !link_exists(&vniname).await {
        if let Err(e) = Command::new("ip")
            .arg("link")
            .arg("add")
            .arg(&vniname)
            .arg("type")
            .arg("vxlan")
            .arg("local")
            .arg(&localstr)
            .arg("dstport")
            .arg("4789")
            .arg("id")
            .arg(&vnistr)
            .arg("nolearning")
            .output()
            .await
        {
            println!("ip link add {vniname} error: {e}");
        }
        if let Err(e) = Command::new("ip")
            .arg("link")
            .arg("set")
            .arg(&vniname)
            .arg("master")
            .arg(&brname)
            .arg("addrgenmode")
            .arg("none")
            .output()
            .await
        {
            println!("ip link set {vniname} master {brname} error: {e}");
        }
        if let Err(e) = Command::new("ip")
            .arg("link")
            .arg("set")
            .arg(&vniname)
            .arg("type")
            .arg("bridge_slave")
            .arg("neigh_suppress")
            .arg("on")
            .arg("learning")
            .arg("off")
            .output()
            .await
        {
            println!("ip link add {vniname} error: {e}");
        }
    }
    if !is_vxlan(&vniname).await {
        return Err(format!("Link {vniname} is not vxlan"));
    }
    if let Err(e) = Command::new("ip")
        .arg("link")
        .arg("set")
        .arg(&vniname)
        .arg("up")
        .output()
        .await
    {
        println!("ip link set {vniname} up error: {e}");
    }
    if let Err(e) = Command::new("ip")
        .arg("link")
        .arg("set")
        .arg(ifname)
        .arg("up")
        .output()
        .await
    {
        println!("ip link set {ifname} up error: {e}");
    }
    if let Err(e) = Command::new("ip")
        .arg("link")
        .arg("set")
        .arg(ifname)
        .arg("master")
        .arg(&brname)
        .output()
        .await
    {
        println!("ip link set {ifname} master {brname} error: {e}");
    }

    Ok(())
}

pub async fn update_vni(vni: u32, local: &Option<Ipv6Addr>) -> Result<(), String> {
    println!("Update VNI {vni}: local={local:?}");
    let vniname = format!("vni{vni}");
    let localstr = if let Some(local) = local {
        format!("{local}")
    } else {
        let l1 = format!("{:02x}", vni >> 16);
        let l2 = format!("{:x}", vni & 0xffffu32);
        format!("::ffff:7f{l1}:{l2}")
    };
    if !link_exists(&vniname).await {
        return Err(format!("Link {vniname} not exists"));
    }
    if let Err(e) = Command::new("ip")
        .arg("link")
        .arg("change")
        .arg(&vniname)
        .arg("type")
        .arg("vxlan")
        .arg("local")
        .arg(&localstr)
        .output()
        .await
    {
        println!("ip link change {vniname} local {localstr} error: {e}");
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
    if let Err(e) = Command::new("ipset")
        .arg("create")
        .arg("gvls-neighs")
        .arg("hash:net")
        .arg("family")
        .arg("inet6")
        .arg("hashsize")
        .arg("1024")
        .arg("maxelem")
        .arg("65536")
        .output()
        .await
    {
        println!("ipset init error: {e}");
    }
    Ok(())
}

pub async fn add_neigh(neigh: &Ipv6Addr) -> Result<(), String> {
    if let Err(e) = Command::new("ipset")
        .arg("add")
        .arg("gvls-neighs")
        .arg(format!("{neigh}"))
        .output()
        .await
    {
        println!("ipset add {neigh} error: {e}");
    }
    Ok(())
}

pub async fn del_neigh(neigh: &Ipv6Addr) -> Result<(), String> {
    if let Err(e) = Command::new("ipset")
        .arg("del")
        .arg("gvls-neighs")
        .arg(format!("{neigh}"))
        .output()
        .await
    {
        println!("ipset del {neigh} error: {e}");
    }
    Ok(())
}
