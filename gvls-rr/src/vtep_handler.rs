// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::collections::HashSet;
use std::net::Ipv6Addr;
use std::time::{Duration, Instant};

use remoc::rch;
use tokio::sync::mpsc;

use libgvls::{
    HELLO_INTERVAL, HELLO_TIMEOUT, HelloMsg, NeighsUpdatedMsg, RegisterVtepRepMsg, RrRchMsg,
    VtepRchMsg,
};

use crate::{AuthVtepReq, RrLchMsg, UpdateNeighsMsg, VtepExitMsg, VtepLchMsg, VtepRegisteredMsg};

pub struct VtepHandler {
    addr: Ipv6Addr,
    name: String,
    hello_next: Instant,
    hello_last: Instant,
    neighs_sent: HashSet<Ipv6Addr>,
    tx_rch: rch::base::Sender<VtepRchMsg>,
    rx_rch: rch::base::Receiver<RrRchMsg>,
    tx_lch: mpsc::Sender<RrLchMsg>,
    rx_lch: mpsc::Receiver<VtepLchMsg>,
}

impl VtepHandler {
    pub fn new(
        addr: Ipv6Addr,
        tx_rch: rch::base::Sender<VtepRchMsg>,
        rx_rch: rch::base::Receiver<RrRchMsg>,
        tx_lch: mpsc::Sender<RrLchMsg>,
        rx_lch: mpsc::Receiver<VtepLchMsg>,
    ) -> Self {
        Self {
            addr,
            name: String::new(),
            hello_next: Instant::now(),
            hello_last: Instant::now(),
            neighs_sent: HashSet::<Ipv6Addr>::new(),
            tx_rch,
            rx_rch,
            tx_lch,
            rx_lch,
        }
    }

    async fn send_hello(&mut self) -> Result<(), String> {
        if Instant::now() < self.hello_next {
            return Ok(());
        }
        let req = VtepRchMsg::Hello(HelloMsg {});
        println!("Sending hello {} {}", self.name, self.addr);
        if let Err(e) = self.tx_rch.send(req).await {
            return Err(format!("Send hello error: {e}"));
        }
        println!("Sent hello {} {}", self.name, self.addr);
        self.hello_next = Instant::now() + Duration::from_secs(HELLO_INTERVAL);
        Ok(())
    }

    fn hello_timeout(&self) -> bool {
        if self.hello_last < Instant::now() - Duration::from_secs(HELLO_TIMEOUT) {
            true
        } else {
            false
        }
    }

    async fn update_neighs(&mut self, msg: UpdateNeighsMsg) -> Result<(), String> {
        if msg.neighs == self.neighs_sent {
            return Ok(());
        }
        let (neigh_tx, neigh_rx) = rch::mpsc::channel(1);
        let rmsg = VtepRchMsg::NeighsUpdated(NeighsUpdatedMsg { neigh_rx: neigh_rx });
        if let Err(e) = self.tx_rch.send(rmsg).await {
            println!("Send neighs updated {} {} error: {e}", self.name, self.addr);
        }
        for neigh in &msg.neighs {
            if let Err(e) = neigh_tx.send(*neigh).await {
                println!("Send neigh error: {e}");
                return Ok(());
            }
        }
        self.neighs_sent = msg.neighs;
        Ok(())
    }

    async fn rch(
        &mut self,
        msg: Result<Option<RrRchMsg>, rch::base::RecvError>,
    ) -> Result<(), String> {
        match msg {
            Ok(Some(RrRchMsg::Hello(_))) => {
                println!("Received hello {} {}", self.name, self.addr);
                self.hello_last = Instant::now();
                Ok(())
            }
            Ok(Some(RrRchMsg::RegisterVtepReq(_))) => {
                // RegisterVtepReq は run で処理する最初のみ
                println!("Unexpected RegisterVtepReq after VTEP registration");
                Ok(())
            }
            Ok(Some(RrRchMsg::VtepAdded(_))) => {
                println!("Unexpected VtepAdded received from VTEP");
                Ok(())
            }
            Ok(Some(RrRchMsg::VtepDeleted(_))) => {
                println!("Unexpected VtepDeleted received from VTEP");
                Ok(())
            }
            Ok(Some(RrRchMsg::VniAdded(_))) => {
                println!("Unexpected VniAdded received from VTEP");
                Ok(())
            }
            Ok(Some(RrRchMsg::VniDeleted(_))) => {
                println!("Unexpected VniDeleted received from VTEP");
                Ok(())
            }
            Ok(Some(RrRchMsg::VtepVniModified(_))) => {
                println!("Unexpected VtepVniModified received from VTEP");
                Ok(())
            }
            Ok(None) => Err(format!("Received none rch")),
            Err(e) => Err(format!("Receive error: {e}")),
        }
    }

    async fn lch(&mut self, msg: Option<VtepLchMsg>) -> Result<(), String> {
        match msg {
            Some(VtepLchMsg::UpdateNeighs(msg)) => self.update_neighs(msg).await,
            None => Err(format!("Received none lch")),
        }
    }

    pub async fn run(&mut self) {
        // RegisterVtepReq
        let rreq = if let Ok(Some(RrRchMsg::RegisterVtepReq(rreq))) = self.rx_rch.recv().await {
            rreq
        } else {
            println!("First msg not RegisterVtepReq");
            return;
        };

        // AuthVtepReq
        let (rep_tx, mut rep_rx) = mpsc::channel(1);
        let lreq = RrLchMsg::AuthVtep(AuthVtepReq {
            name: rreq.name,
            password: rreq.password,
            rem_addr: self.addr.clone(),
            rep_tx,
        });
        println!("Sending AuthVtepReq {} {}", self.name, self.addr);
        if let Err(e) = self.tx_lch.send(lreq).await {
            println!("Send lch error: {e}");
            return;
        }
        println!("Sent AuthVtepReq {} {}", self.name, self.addr);

        // AuthVtepRep
        println!("Receiving AuthVtepRep {} {}", self.name, self.addr);
        let lrep = if let Some(lrep) = rep_rx.recv().await {
            lrep
        } else {
            println!("Recv lch none");
            return;
        };
        println!("Received AuthVtepRep {} {}", self.name, self.addr);
        match lrep.vtep_name {
            Ok(vtep_name) => {
                self.name = vtep_name;
            }
            Err(e) => {
                println!("Auth failed: {e}");
                return;
            }
        }

        // RegisterVtepRep
        let (neigh_tx, neigh_rx) = rch::mpsc::channel(1);
        let rrep = RegisterVtepRepMsg {
            bgp_pass: lrep.bgp_pass,
            neigh_rx: neigh_rx,
        };
        if let Err(e) = rreq.rep_tx.send(rrep).await {
            println!("Send rch error: {e}");
            return;
        }
        for neigh in &lrep.neighs {
            if let Err(e) = neigh_tx.send(*neigh).await {
                println!("Send neigh error: {e}");
                return;
            }
        }
        drop(neigh_tx);
        self.neighs_sent = lrep.neighs;

        // VtepRegistered
        let lmsg = RrLchMsg::VtepRegistered(VtepRegisteredMsg {
            name: self.name.clone(),
            rem_addr: self.addr.clone(),
        });
        println!("Sending VtepRegistered {} {}", self.name, self.addr);
        if let Err(e) = self.tx_lch.send(lmsg).await {
            println!("Send lch error: {e}");
            return;
        }
        println!("Sent VtepRegistered {} {}", self.name, self.addr);

        println!("Registered: {} {}", self.name, self.addr);
        self.hello_last = Instant::now();

        let mut timer = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                msg = self.rx_rch.recv() => {
                    if let Err(e) = self.rch(msg).await {
                        println!("rch error: {e}");
                        break;
                    }
                },
                msg = self.rx_lch.recv() => {
                    if let Err(e) = self.lch(msg).await {
                        println!("lch error: {e}");
                        break;
                    }
                },
                _ = timer.tick() => {
                    if let Err(e) = self.send_hello().await {
                        println!("hello error: {e}");
                        break;
                    }
                    if self.hello_timeout() {
                        println!("hello timeout");
                        break;
                    }
                }
            }
        }

        // VtepExit
        println!("Sending VtepExit {} {}", self.name, self.addr);
        let lmsg = RrLchMsg::VtepExit(VtepExitMsg {
            name: self.name.clone(),
            rem_addr: self.addr.clone(),
        });
        let _ = self.tx_lch.send(lmsg).await;
        println!("Sent VtepExit {} {}", self.name, self.addr);

        println!("Exit: {} {}", self.name, self.addr);
    }
}
