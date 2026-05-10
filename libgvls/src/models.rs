// SPDX-License-Identifier: MIT
// Copyright(c) 2026 Masakazu Asama

use std::collections::HashSet;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Permission {
    Admin,
    Free,
    Pro,
}

impl Permission {
    pub fn limits(self) -> Option<(usize, usize)> {
        match self {
            Permission::Admin => None,
            Permission::Free => Some((1, 3)),
            Permission::Pro => Some((10, 30)),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Account {
    pub id: i32,
    pub mail_addr: String,
    pub password: String,
    pub perm: Permission,
    pub vnis: HashSet<i32>,
    pub vteps: HashSet<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Rr {
    pub id: i32,
    pub name: String,
    pub password: String,
    #[serde(skip)]
    pub ipv4_addr: Option<Ipv4Addr>,
    #[serde(skip)]
    pub last_update: Option<Instant>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Vtep {
    pub id: i32,
    pub name: String,
    pub description: String,
    pub password: String,
    pub bgp_pass: String,
    pub account_id: i32,
    pub vnis: HashSet<i32>,
    #[serde(skip)]
    pub ipv6_addr: Option<Ipv6Addr>,
    #[serde(skip)]
    pub last_update: Option<Instant>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Vni {
    pub id: i32,
    pub vni: i32,
    pub description: String,
    pub account_id: i32,
    pub vteps: HashSet<String>,
}
