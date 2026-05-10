// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::collections::HashSet;
use std::net::Ipv6Addr;

use tokio::sync::mpsc;

use libgvls::{BgpOps, get_ifindex};

use crate::{
    Config, LocAddrChangedMsg, RemAddrChangedMsg, RrHandler, RrLchMsg, RtnlHandler,
    UpdateNeighsMsg, VtepLchMsg, VtepRegisteredMsg, add_neigh, del_neigh, init_neighs, init_vni,
    update_bgp, update_vni,
};

#[derive(Debug)]
pub struct Context {
    vtep_name: String,
    vtep_pass: String,
    src_ifname: String,
    src_ifindex: u32,
    rr_hosts: [String; 2],
    rr_ports: [u16; 2],
    vnis: Vec<(u32, String)>,
    bgp_ops: BgpOps,
    bgp_asnum: u32,
    bgp_pass: [String; 2],
    //
    loc_addr: Option<Ipv6Addr>,
    rem_addrs: [Option<Ipv6Addr>; 2],
    neighs_active: HashSet<Ipv6Addr>,
    neighs_allowed_by_rr: [HashSet<Ipv6Addr>; 2],
    //
    tx_lch: mpsc::Sender<VtepLchMsg>,
    rx_lch: mpsc::Receiver<VtepLchMsg>,
    tx_rr_lch: mpsc::Sender<RrLchMsg>,
    rx_rr_lch: mpsc::Receiver<RrLchMsg>,
    rr_tx_lchs: [Option<mpsc::Sender<RrLchMsg>>; 2],
}

impl Context {
    pub fn from_conf(conf: Config) -> Result<Self, String> {
        if conf.vtep_name == "" {
            return Err(format!("GVLS_VTEP_NAME required"));
        }
        if conf.vtep_pass == "" {
            return Err(format!("GVLS_VTEP_PASS required"));
        }
        let (tx_lch, rx_lch) = mpsc::channel(1);
        let (tx_rr_lch, rx_rr_lch) = mpsc::channel(1);
        Ok(Self {
            vtep_name: conf.vtep_name,
            vtep_pass: conf.vtep_pass,
            src_ifname: conf.src_ifname,
            src_ifindex: 0,
            rr_hosts: conf.rr_hosts,
            rr_ports: conf.rr_ports,
            vnis: conf.vnis,
            bgp_ops: conf.bgp_ops,
            bgp_asnum: conf.bgp_asnum,
            bgp_pass: [String::new(), String::new()],
            //
            loc_addr: None,
            rem_addrs: [None, None],
            neighs_active: HashSet::<Ipv6Addr>::new(),
            neighs_allowed_by_rr: [HashSet::<Ipv6Addr>::new(), HashSet::<Ipv6Addr>::new()],
            //
            tx_lch,
            rx_lch,
            tx_rr_lch,
            rx_rr_lch,
            rr_tx_lchs: [None, None],
        })
    }

    async fn init_src_ifindex(&mut self) -> Result<(), String> {
        self.src_ifindex = get_ifindex(&self.src_ifname).await?;
        Ok(())
    }

    async fn init_vnis(&self) -> Result<(), String> {
        for (vni, ifname) in &self.vnis {
            if let Err(e) = init_vni(*vni, ifname, &self.loc_addr).await {
                return Err(format!("Setup VNI {vni}:{ifname} error: {e}"));
            }
        }
        Ok(())
    }

    async fn sync_neighs(&mut self) -> Result<(), String> {
        let mut neighs_allowed = HashSet::<Ipv6Addr>::new();
        for rr_neighs in &self.neighs_allowed_by_rr {
            neighs_allowed.extend(rr_neighs.iter().copied());
        }

        let neighs_to_del = self
            .neighs_active
            .difference(&neighs_allowed)
            .copied()
            .collect::<Vec<_>>();
        let neighs_to_add = neighs_allowed
            .difference(&self.neighs_active)
            .copied()
            .collect::<Vec<_>>();

        if !neighs_to_del.is_empty() || !neighs_to_add.is_empty() {
            println!(
                "Sync neighbors: add={:?} del={:?}",
                neighs_to_add, neighs_to_del
            );
        }
        for neigh_del in &neighs_to_del {
            if let Err(e) = del_neigh(neigh_del).await {
                println!("Delete neighbor {neigh_del} failed: {e}");
            }
        }
        for neigh_add in &neighs_to_add {
            if let Err(e) = add_neigh(neigh_add).await {
                println!("Add neighbor {neigh_add} failed: {e}");
            }
        }
        self.neighs_active = neighs_allowed;
        Ok(())
    }

    async fn vtep_registered(&mut self, msg: VtepRegisteredMsg) -> Result<(), String> {
        if msg.rr_index >= self.neighs_allowed_by_rr.len() {
            println!("VTEP registration from invalid RR index: {}", msg.rr_index);
            return Ok(());
        }
        println!(
            "Registered to RR #{}: neighs={}",
            msg.rr_index + 1,
            msg.neighs.len()
        );
        self.bgp_pass[msg.rr_index] = msg.bgp_pass;
        self.neighs_allowed_by_rr[msg.rr_index] = msg.neighs;
        if let Err(e) = self.sync_neighs().await {
            println!("Sync neighbors failed: {e}");
        }
        Ok(())
    }

    async fn loc_addr_changed(&mut self, msg: LocAddrChangedMsg) -> Result<(), String> {
        println!(
            "Local address changed: {:?} -> {:?}",
            self.loc_addr, msg.loc_addr
        );
        for (vni, ifname) in &self.vnis {
            if let Err(e) = update_vni(*vni, &msg.loc_addr).await {
                println!("Update VNI {vni}:{ifname} failed: {e}");
            }
        }
        for i in 0..2 {
            if let Err(e) = update_bgp(
                &self.loc_addr,
                &msg.loc_addr,
                &self.rem_addrs[i],
                &self.rem_addrs[i],
                &self.bgp_ops,
                self.bgp_asnum,
                &self.rr_hosts[i],
                &self.bgp_pass[i],
            )
            .await
            {
                println!("Update BGP for RR #{} failed: {e}", i + 1);
            }
        }
        for i in 0..2 {
            if let Some(rr_tx_lch) = &self.rr_tx_lchs[i] {
                let _ = rr_tx_lch.send(RrLchMsg::LocAddrChanged(msg.clone())).await;
            }
        }
        self.loc_addr = msg.loc_addr;
        Ok(())
    }

    async fn rem_addr_changed(&mut self, msg: RemAddrChangedMsg) -> Result<(), String> {
        if msg.rr_index >= self.rem_addrs.len() {
            println!(
                "Remote address change from invalid RR index: {}",
                msg.rr_index
            );
            return Ok(());
        }
        println!(
            "RR #{} remote address changed: {:?} -> {:?}",
            msg.rr_index + 1,
            self.rem_addrs[msg.rr_index],
            msg.rem_addr
        );
        if let Err(e) = update_bgp(
            &self.loc_addr,
            &self.loc_addr,
            &self.rem_addrs[msg.rr_index],
            &msg.rem_addr,
            &self.bgp_ops,
            self.bgp_asnum,
            &self.rr_hosts[msg.rr_index],
            &self.bgp_pass[msg.rr_index],
        )
        .await
        {
            println!("Update BGP for RR #{} failed: {e}", msg.rr_index + 1);
        }
        self.rem_addrs[msg.rr_index] = msg.rem_addr;
        Ok(())
    }

    async fn update_neighs(&mut self, msg: UpdateNeighsMsg) -> Result<(), String> {
        if msg.rr_index >= self.neighs_allowed_by_rr.len() {
            println!("Neighbor update from invalid RR index: {}", msg.rr_index);
            return Ok(());
        }
        println!(
            "Received neighbor update from RR #{}: neighs={}",
            msg.rr_index + 1,
            msg.neighs.len()
        );
        self.neighs_allowed_by_rr[msg.rr_index] = msg.neighs;
        if let Err(e) = self.sync_neighs().await {
            println!("Sync neighbors failed: {e}");
        }
        Ok(())
    }

    async fn lch(&mut self, msg: Option<VtepLchMsg>) -> Result<(), String> {
        match msg {
            Some(VtepLchMsg::LocAddrChanged(msg)) => self.loc_addr_changed(msg).await,
            None => Err(format!("Received none lch")),
        }
    }

    async fn rr_lch(&mut self, msg: Option<RrLchMsg>) -> Result<(), String> {
        match msg {
            Some(RrLchMsg::VtepRegistered(msg)) => self.vtep_registered(msg).await,
            Some(RrLchMsg::LocAddrChanged(_)) => {
                println!("Unexpected LocAddrChanged received on context RR channel");
                Ok(())
            }
            Some(RrLchMsg::RemAddrChanged(msg)) => self.rem_addr_changed(msg).await,
            Some(RrLchMsg::UpdateNeighs(msg)) => self.update_neighs(msg).await,
            None => Err(format!("Received none rr lch")),
        }
    }

    pub async fn run(&mut self) {
        if let Err(e) = self.init_src_ifindex().await {
            println!("Get source interface index failed: {e}");
            return;
        }
        self.bgp_ops.init(self.bgp_asnum).await;
        if let Err(e) = self.init_vnis().await {
            println!("VNIs setup error: {e}");
            return;
        }
        if let Err(e) = init_neighs().await {
            println!("Init neighs error: {e}");
            return;
        }
        let mut rtnl_handler = RtnlHandler::new(
            self.src_ifname.clone(),
            self.src_ifindex,
            self.tx_lch.clone(),
        );
        tokio::spawn(async move {
            rtnl_handler.run().await;
        });
        for i in 0..2 {
            let (rr_tx_lch, rr_rx_lch) = mpsc::channel(1);
            let mut rr_handler = RrHandler::new(
                self.vtep_name.clone(),
                self.vtep_pass.clone(),
                i,
                self.rr_hosts[i].clone(),
                self.rr_ports[i],
                self.tx_rr_lch.clone(),
                rr_rx_lch,
            );
            tokio::spawn(async move {
                rr_handler.run().await;
            });
            self.rr_tx_lchs[i] = Some(rr_tx_lch);
        }
        loop {
            tokio::select! {
                msg = self.rx_lch.recv() => {
                    if let Err(e) = self.lch(msg).await {
                        println!("lch error: {e}");
                        break;
                    }
                }
                msg = self.rx_rr_lch.recv() => {
                    if let Err(e) = self.rr_lch(msg).await {
                        println!("rr lch error: {e}");
                        break;
                    }
                }
            };
        }
        println!("context exit");
    }
}
