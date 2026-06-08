// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::collections::HashSet;
use std::net::Ipv6Addr;
use std::time::Duration;

use remoc::rch;
use tokio::sync::mpsc;
use tokio::time::sleep;

use libgvls::{
    AddVniRepMsg, AddVniReqMsg, BgpOps, DelVniRepMsg, DelVniReqMsg, UdsRchListener,
    VtepCtrlVtepRchMsg, VtepVtepCtrlRchMsg, exec, get_ifindex,
};

use crate::{
    Config, LocAddrChangedMsg, RemAddrChangedMsg, RrHandler, RrLchMsg, RtnlHandler,
    UpdateNeighsMsg, VtepLchMsg, VtepRegisteredMsg, add_neigh, add_vni, del_neigh, del_vni,
    init_neighs, update_bgp, update_vni,
};

#[derive(Debug)]
pub struct Context {
    vtep_name: String,
    vtep_pass: String,
    src_ifname: String,
    src_ifindex: u32,
    rr_hosts: [String; 2],
    rr_ports: [u16; 2],
    rch_path: String,
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
        Ok(Self {
            vtep_name: conf.vtep_name,
            vtep_pass: conf.vtep_pass,
            src_ifname: conf.src_ifname,
            src_ifindex: 0,
            rr_hosts: conf.rr_hosts,
            rr_ports: conf.rr_ports,
            rch_path: conf.rch_path,
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
            rr_tx_lchs: [None, None],
        })
    }

    async fn init_src_ifindex(&mut self) -> Result<(), String> {
        self.src_ifindex = get_ifindex(&self.src_ifname).await?;
        Ok(())
    }

    async fn init_vnis(&self) -> Result<(), String> {
        for (vni, ifname) in &self.vnis {
            if let Err(e) = add_vni(*vni, ifname, &self.loc_addr).await {
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
            if let Err(e) = update_vni(*vni, &self.loc_addr, &msg.loc_addr).await {
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
            Some(VtepLchMsg::VtepRegistered(msg)) => self.vtep_registered(msg).await,
            Some(VtepLchMsg::LocAddrChanged(msg)) => self.loc_addr_changed(msg).await,
            Some(VtepLchMsg::RemAddrChanged(msg)) => self.rem_addr_changed(msg).await,
            Some(VtepLchMsg::UpdateNeighs(msg)) => self.update_neighs(msg).await,
            None => Err(format!("Received none lch")),
        }
    }

    async fn wait(&self) -> Result<(), String> {
        let mut bgp_ready: bool = false;
        let mut src_if_ready: bool = false;
        for _ in 0..60 {
            if !bgp_ready {
                if let Ok(_) = self.bgp_ops.ready().await {
                    bgp_ready = true;
                }
            }
            if !src_if_ready {
                if let Ok(_) = get_ifindex(&self.src_ifname).await {
                    src_if_ready = true;
                }
            }
            if bgp_ready && src_if_ready {
                return Ok(());
            }
            sleep(Duration::from_secs(1)).await;
        }
        Err(format!(
            "bgp_ready = {bgp_ready} src_if_ready = {src_if_ready}"
        ))
    }

    async fn add_vni(
        &mut self,
        req: AddVniReqMsg,
        mut tx_rch: rch::base::Sender<VtepVtepCtrlRchMsg>,
    ) -> Result<(), String> {
        for (vni, _) in &self.vnis {
            if req.vni == *vni {
                let err = Err(format!("VNI {vni} is already exists"));
                let rep = VtepVtepCtrlRchMsg::AddVniRep(AddVniRepMsg {
                    result: err.clone(),
                });
                let _ = tx_rch.send(rep).await;
                return err;
            }
        }
        if let Err(e) = add_vni(req.vni, &req.ifname, &self.loc_addr).await {
            let err = Err(format!("Add VNI {}:{} error: {e}", req.vni, req.ifname));
            let rep = VtepVtepCtrlRchMsg::AddVniRep(AddVniRepMsg {
                result: err.clone(),
            });
            let _ = tx_rch.send(rep).await;
            return err;
        }
        self.vnis.push((req.vni, req.ifname));
        let rep = VtepVtepCtrlRchMsg::AddVniRep(AddVniRepMsg { result: Ok(()) });
        let _ = tx_rch.send(rep).await;
        Ok(())
    }

    async fn del_vni(
        &mut self,
        req: DelVniReqMsg,
        mut tx_rch: rch::base::Sender<VtepVtepCtrlRchMsg>,
    ) -> Result<(), String> {
        let mut vnis = Vec::<(u32, String)>::new();
        let mut found = false;
        for (vni, ifname) in &self.vnis {
            if req.vni == *vni {
                found = true;
            } else {
                vnis.push((*vni, ifname.clone()));
            }
        }
        if !found {
            let err = Err(format!("VNI {} is not exist", req.vni));
            let rep = VtepVtepCtrlRchMsg::DelVniRep(DelVniRepMsg {
                result: err.clone(),
            });
            let _ = tx_rch.send(rep).await;
            return err;
        }
        if let Err(e) = del_vni(req.vni).await {
            let err = Err(format!("Delete VNI {} error: {e}", req.vni));
            let rep = VtepVtepCtrlRchMsg::DelVniRep(DelVniRepMsg {
                result: err.clone(),
            });
            let _ = tx_rch.send(rep).await;
            return err;
        }
        self.vnis = vnis;
        let rep = VtepVtepCtrlRchMsg::DelVniRep(DelVniRepMsg { result: Ok(()) });
        let _ = tx_rch.send(rep).await;
        Ok(())
    }

    async fn vtep_ctrl(
        &mut self,
        tx_rch: rch::base::Sender<VtepVtepCtrlRchMsg>,
        mut rx_rch: rch::base::Receiver<VtepCtrlVtepRchMsg>,
    ) -> Result<(), String> {
        match rx_rch.recv().await {
            Ok(Some(VtepCtrlVtepRchMsg::AddVniReq(req))) => self.add_vni(req, tx_rch).await,
            Ok(Some(VtepCtrlVtepRchMsg::DelVniReq(req))) => self.del_vni(req, tx_rch).await,
            Ok(None) => Err(format!("Received none rch")),
            Err(e) => Err(format!("Receive error: {e}")),
        }
    }

    pub async fn run(&mut self) {
        if let Err(e) = self.wait().await {
            println!("Waiting BGP backend and source interface error: {e}");
            return;
        }
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

        // RtnlHandler
        let mut rtnl_handler = RtnlHandler::new(
            self.src_ifname.clone(),
            self.src_ifindex,
            self.tx_lch.clone(),
        );
        tokio::spawn(async move {
            rtnl_handler.run().await;
        });

        // RrHandler
        for i in 0..2 {
            let (rr_tx_lch, rr_rx_lch) = mpsc::channel(1);
            let mut rr_handler = RrHandler::new(
                self.vtep_name.clone(),
                self.vtep_pass.clone(),
                i,
                self.rr_hosts[i].clone(),
                self.rr_ports[i],
                self.tx_lch.clone(),
                rr_rx_lch,
            );
            tokio::spawn(async move {
                rr_handler.run().await;
            });
            self.rr_tx_lchs[i] = Some(rr_tx_lch);
        }

        // UdsRchListener
        let mut rch_listener = match UdsRchListener::new(self.rch_path.clone()).await {
            Ok(rch_listener) => rch_listener,
            Err(e) => {
                println!("Rch listener new error: {e}");
                return;
            }
        };
        exec(vec!["chmod", "600", &self.rch_path]).await;

        loop {
            tokio::select! {
                ret = rch_listener.rch_accept::<VtepVtepCtrlRchMsg, VtepCtrlVtepRchMsg>() => {
                    match ret {
                        Ok((tx_rch, rx_rch)) => {
                            if let Err(e) = self.vtep_ctrl(tx_rch, rx_rch).await {
                                println!("VTEP ctrl error: {e}");
                            }
                        }
                        Err(e) => {
                            println!("Rch accept error: {e}");
                        }
                    }
                }
                msg = self.rx_lch.recv() => {
                    if let Err(e) = self.lch(msg).await {
                        println!("lch error: {e}");
                        break;
                    }
                }
            };
        }

        println!("context exit");
    }
}
