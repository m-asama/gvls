// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::net::{IpAddr, Ipv6Addr};

use futures_util::stream::TryStreamExt;
use netlink_packet_route::AddressFamily;
use netlink_packet_route::address::{
    AddressAttribute::Address, AddressAttribute::Flags, AddressFlags, AddressMessage, AddressScope,
};
use netlink_packet_route::link::{InfoKind, LinkAttribute, LinkInfo, LinkMessage};
use rtnetlink::new_connection;

pub async fn get_link(ifname: &str) -> Result<LinkMessage, String> {
    let (connection, handle) = match new_connection() {
        Ok((connection, handle, _)) => (connection, handle),
        Err(e) => return Err(format!("Get new connection error: {e}")),
    };
    tokio::spawn(connection);
    let mut links = handle.link().get().match_name(ifname.to_string()).execute();
    match links.try_next().await {
        Ok(Some(link)) => Ok(link),
        _ => return Err(format!("Link {ifname} not found")),
    }
}

pub async fn link_exists(ifname: &str) -> bool {
    match get_link(ifname).await {
        Ok(_) => true,
        Err(_) => false,
    }
}

pub async fn is_bridge(ifname: &str) -> bool {
    let link = match get_link(ifname).await {
        Ok(link) => link,
        Err(_) => return false,
    };
    for attr in &link.attributes {
        if let LinkAttribute::LinkInfo(lis) = attr {
            for li in lis {
                if let LinkInfo::Kind(InfoKind::Bridge) = li {
                    return true;
                }
            }
        }
    }
    false
}

pub async fn is_vxlan(ifname: &str) -> bool {
    let link = match get_link(ifname).await {
        Ok(link) => link,
        Err(_) => return false,
    };
    for attr in &link.attributes {
        if let LinkAttribute::LinkInfo(lis) = attr {
            for li in lis {
                if let LinkInfo::Kind(InfoKind::Vxlan) = li {
                    return true;
                }
            }
        }
    }
    false
}

pub async fn get_ifindex(ifname: &str) -> Result<u32, String> {
    let link = get_link(ifname).await?;
    Ok(link.header.index)
}

pub async fn get_ipv6_addrs(ifname: &str) -> Result<Vec<Ipv6Addr>, String> {
    let (connection, handle) = match new_connection() {
        Ok((connection, handle, _)) => (connection, handle),
        Err(e) => return Err(format!("Get new connection error: {e}")),
    };
    tokio::spawn(connection);
    let mut links = handle.link().get().match_name(ifname.to_string()).execute();
    let link = match links.try_next().await {
        Ok(Some(link)) => link,
        _ => return Err(format!("Link {ifname} not found")),
    };
    let mut addresses = handle
        .address()
        .get()
        .set_link_index_filter(link.header.index)
        .execute();
    let tmpflag = AddressFlags::from_name("Secondary").unwrap();
    let mut addrs = Vec::<Ipv6Addr>::new();
    while let Ok(Some(msg)) = addresses.try_next().await {
        if msg.header.family != AddressFamily::Inet6 {
            continue;
        }
        if msg.header.scope != AddressScope::Universe {
            continue;
        }
        let mut addr: Option<Ipv6Addr> = None;
        let mut tmpaddr = false;
        for attr in &msg.attributes {
            if let Address(IpAddr::V6(a)) = attr {
                addr = Some(*a);
            }
            if let Flags(flags) = attr {
                if flags.contains(tmpflag) {
                    tmpaddr = true;
                }
            }
        }
        if tmpaddr {
            continue;
        }
        if let Some(addr) = addr {
            addrs.push(addr);
        }
    }
    if addrs.len() > 0 {
        Ok(addrs)
    } else {
        Err(format!("IPv6 address not found"))
    }
}

pub fn parse_ipv6_addr(msg: AddressMessage) -> Option<Ipv6Addr> {
    if msg.header.family != AddressFamily::Inet6 {
        return None;
    }
    if msg.header.scope != AddressScope::Universe {
        return None;
    }
    let tmpflag = AddressFlags::from_name("Secondary").unwrap();
    let mut addr: Option<Ipv6Addr> = None;
    let mut tmpaddr = false;
    for attr in &msg.attributes {
        if let Address(IpAddr::V6(a)) = attr {
            addr = Some(*a);
        }
        if let Flags(flags) = attr {
            if flags.contains(tmpflag) {
                tmpaddr = true;
            }
        }
    }
    if tmpaddr {
        return None;
    }
    addr
}
