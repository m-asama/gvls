// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

use remoc::rch;
use tokio::sync::mpsc;

use libgvls::{
    HELLO_INTERVAL, HELLO_TIMEOUT, HelloMsg, RegisterRrReqMsg, UiRchMsg, UiRrRchMsg, Vni,
    VniAddedMsg, VniDeletedMsg, Vtep, VtepAddedMsg, VtepDeletedMsg, VtepStateUpdatedMsg,
    VtepVniModifiedMsg, rch_connect_addr,
};

use crate::{
    AddVniMsg, AddVtepMsg, DelVniMsg, DelVtepMsg, ModVtepVniMsg, RrLchMsg, RrRegisteredMsg,
    UiLchMsg, UpdateVtepStateMsg,
};

pub struct UiHandler {
    ui_addr: Ipv4Addr,
    ui_port: u16,
    rr_name: String,
    rr_pass: String,
    conn_try_count: u32,
    conn_try_next: Instant,
    hello_next: Instant,
    hello_last: Instant,
    tx_rch: Option<rch::base::Sender<UiRchMsg>>,
    rx_rch: Option<rch::base::Receiver<UiRrRchMsg>>,
    tx_lch: mpsc::Sender<RrLchMsg>,
    rx_lch: mpsc::Receiver<UiLchMsg>,
}

impl UiHandler {
    pub fn new(
        ui_addr: Ipv4Addr,
        ui_port: u16,
        rr_name: String,
        rr_pass: String,
        tx_lch: mpsc::Sender<RrLchMsg>,
        rx_lch: mpsc::Receiver<UiLchMsg>,
    ) -> Self {
        Self {
            ui_addr,
            ui_port,
            rr_name,
            rr_pass,
            conn_try_count: 0,
            conn_try_next: Instant::now(),
            hello_next: Instant::now(),
            hello_last: Instant::now(),
            tx_rch: None,
            rx_rch: None,
            tx_lch,
            rx_lch,
        }
    }

    fn reset(&mut self) {
        self.tx_rch = None;
        self.rx_rch = None;
        self.conn_try_count = 0;
        self.conn_try_next = Instant::now() + Duration::from_secs(60);
    }

    fn retry(&mut self) {
        self.tx_rch = None;
        self.rx_rch = None;
        self.conn_try_count += 1;
        let n = if self.conn_try_count < 10 {
            self.conn_try_count
        } else {
            10
        };
        self.conn_try_next = Instant::now() + Duration::from_secs(2u64.pow(n));
    }

    async fn send_hello(&mut self) -> Result<(), String> {
        if Instant::now() < self.hello_next {
            return Ok(());
        }
        if let Some(tx_rch) = &mut self.tx_rch {
            let req = UiRchMsg::Hello(HelloMsg {});
            println!("Sending hello {}", self.ui_addr);
            if let Err(e) = tx_rch.send(req).await {
                return Err(format!("Send hello error: {e}"));
            }
            println!("Sent hello {}", self.ui_addr);
            self.hello_next = Instant::now() + Duration::from_secs(HELLO_INTERVAL);
        }
        Ok(())
    }

    fn hello_timeout(&self) -> bool {
        if self.hello_last < Instant::now() - Duration::from_secs(HELLO_TIMEOUT) {
            true
        } else {
            false
        }
    }

    async fn conn_try(&mut self) {
        if Instant::now() < self.conn_try_next {
            return;
        }

        println!("Starting gvls-ui connection attempt ({})", self.ui_addr);

        // rch connect
        let ui_addr = IpAddr::V4(self.ui_addr.clone());
        let ui_port = self.ui_port;
        let (tx_rch, rx_rch) =
            match rch_connect_addr::<UiRchMsg, UiRrRchMsg>(ui_addr, ui_port).await {
                Ok((tx_rch, rx_rch)) => (tx_rch, rx_rch),
                Err(e) => {
                    println!("gvls-ui remoc connect ({}) failed: {e}", self.ui_addr);
                    self.retry();
                    return;
                }
            };
        self.tx_rch = Some(tx_rch);
        self.rx_rch = Some(rx_rch);
        self.conn_try_count = 0;
        self.conn_try_next = Instant::now();

        // RegisterRrReq
        let (rep_tx, mut rep_rx) = rch::mpsc::channel(1);
        let req = UiRchMsg::RegisterRrReq(RegisterRrReqMsg {
            name: self.rr_name.clone(),
            password: self.rr_pass.clone(),
            rep_tx,
        });
        println!("Sending RegisterRrReq to gvls-ui ({})", self.ui_addr);
        if let Err(e) = self.tx_rch.as_mut().unwrap().send(req).await {
            println!(
                "Send RegisterRrReq to gvls-ui ({}) failed: {e}",
                self.ui_addr
            );
            self.retry();
            return;
        }
        println!("Sent RegisterRrReq to gvls-ui ({})", self.ui_addr);

        // RegisterRrRep
        println!("Receiving RegisterRrRep from gvls-ui ({})", self.ui_addr);
        let mut rep = match rep_rx.recv().await {
            Ok(Some(rep)) => rep,
            Ok(None) => {
                println!("RegisterRrRep channel closed by gvls-ui ({})", self.ui_addr);
                self.retry();
                return;
            }
            Err(e) => {
                println!(
                    "Receive RegisterRrRep from gvls-ui ({}) failed: {e}",
                    self.ui_addr
                );
                self.retry();
                return;
            }
        };
        println!("Received RegisterRrRep from gvls-ui ({})", self.ui_addr);
        let rr_id = rep.rr_id;
        let mut vteps = HashMap::<String, Vtep>::new();
        while let Ok(Some(vtep)) = rep.vtep_rx.recv().await {
            vteps.insert(vtep.name.clone(), vtep);
        }
        let mut vnis = HashMap::<i32, Vni>::new();
        while let Ok(Some(vni)) = rep.vni_rx.recv().await {
            vnis.insert(vni.vni, vni);
        }
        drop(rep);

        // RrRegistered
        let msg = RrLchMsg::RrRegistered(RrRegisteredMsg { rr_id, vteps, vnis });
        if let Err(e) = self.tx_lch.send(msg).await {
            println!("Send RrRegistered to context failed: {e}");
            self.retry();
            return;
        }

        self.hello_last = Instant::now();
    }

    async fn vtep_added(&mut self, msg: VtepAddedMsg) -> Result<(), String> {
        let lmsg = RrLchMsg::AddVtep(AddVtepMsg { vtep: msg.vtep });
        if let Err(e) = self.tx_lch.send(lmsg).await {
            println!("Send AddVtep error: {e}");
        }
        Ok(())
    }

    async fn vtep_deleted(&mut self, msg: VtepDeletedMsg) -> Result<(), String> {
        let lmsg = RrLchMsg::DelVtep(DelVtepMsg { vtep: msg.vtep });
        if let Err(e) = self.tx_lch.send(lmsg).await {
            println!("Send DelVtep error: {e}");
        }
        Ok(())
    }

    async fn vni_added(&mut self, msg: VniAddedMsg) -> Result<(), String> {
        let lmsg = RrLchMsg::AddVni(AddVniMsg { vni: msg.vni });
        if let Err(e) = self.tx_lch.send(lmsg).await {
            println!("Send AddVni error: {e}");
        }
        Ok(())
    }

    async fn vni_deleted(&mut self, msg: VniDeletedMsg) -> Result<(), String> {
        let lmsg = RrLchMsg::DelVni(DelVniMsg { vni: msg.vni });
        if let Err(e) = self.tx_lch.send(lmsg).await {
            println!("Send DelVni error: {e}");
        }
        Ok(())
    }

    async fn vtep_vni_modified(&mut self, msg: VtepVniModifiedMsg) -> Result<(), String> {
        let lmsg = RrLchMsg::ModVtepVni(ModVtepVniMsg {
            vtep: msg.vtep,
            vni: msg.vni,
        });
        if let Err(e) = self.tx_lch.send(lmsg).await {
            println!("Send ModVtepVni error: {e}");
        }
        Ok(())
    }

    async fn update_vtep_state(&mut self, msg: UpdateVtepStateMsg) -> Result<(), String> {
        if let Some(tx_rch) = &mut self.tx_rch {
            let rmsg = UiRchMsg::VtepStateUpdated(VtepStateUpdatedMsg {
                name: msg.name,
                ipv6_addr: msg.ipv6_addr,
                last_update: msg.last_update,
            });
            if let Err(e) = tx_rch.send(rmsg).await {
                return Err(format!("Send update VTEP error: {e}"));
            }
        }
        Ok(())
    }

    async fn rch(
        &mut self,
        msg: Result<Option<UiRrRchMsg>, rch::base::RecvError>,
    ) -> Result<(), String> {
        match msg {
            Ok(Some(UiRrRchMsg::Hello(_))) => {
                println!("Received hello {}", self.ui_addr);
                self.hello_last = Instant::now();
                Ok(())
            }
            Ok(Some(UiRrRchMsg::VtepAdded(msg))) => self.vtep_added(msg).await,
            Ok(Some(UiRrRchMsg::VtepDeleted(msg))) => self.vtep_deleted(msg).await,
            Ok(Some(UiRrRchMsg::VniAdded(msg))) => self.vni_added(msg).await,
            Ok(Some(UiRrRchMsg::VniDeleted(msg))) => self.vni_deleted(msg).await,
            Ok(Some(UiRrRchMsg::VtepVniModified(msg))) => self.vtep_vni_modified(msg).await,
            Ok(None) => Err(format!("Received none rch")),
            Err(e) => Err(format!("Receive error: {e}")),
        }
    }

    async fn lch(&mut self, msg: Option<UiLchMsg>) -> Result<(), String> {
        match msg {
            Some(UiLchMsg::UpdateVtepState(msg)) => self.update_vtep_state(msg).await,
            None => Err(format!("Received none lch")),
        }
    }

    pub async fn run(&mut self) {
        let mut timer = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                msg = async {
                    match self.rx_rch.as_mut() {
                        Some(rx_rch) => rx_rch.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if let Err(e) = self.rch(msg).await {
                        println!("rch error: {e}");
                        self.reset();
                    }
                },
                msg = self.rx_lch.recv() => {
                    if let Err(e) = self.lch(msg).await {
                        println!("lch error: {e}");
                        break;
                    }
                },
                _ = timer.tick() => {
                    if self.tx_rch.is_none() || self.rx_rch.is_none() {
                        self.conn_try().await;
                    }
                    if self.tx_rch.is_some() {
                        if let Err(e) = self.send_hello().await {
                            println!("hello error: {e}");
                            self.reset();
                        }
                    }
                    if self.rx_rch.is_some() {
                        if self.hello_timeout() {
                            println!("hello timeout");
                            self.reset();
                        }
                    }
                },
            }
        }
    }
}
