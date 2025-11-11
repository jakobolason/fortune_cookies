use leptos::prelude::LeptosOptions;
use std::{
    net::{IpAddr, Ipv4Addr},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH}
};
#[cfg(feature = "ssr")]
use tokio::sync::RwLock;
#[cfg(feature = "ssr")]
use axum;


#[cfg(feature = "ssr")]
pub fn generate_node_id() -> u32 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    (now % 101) as u32
}

#[cfg(feature = "ssr")]
#[derive(Clone, Debug)]
pub struct AppState {
    pub leptos_options: LeptosOptions,
    pub leader: Arc<RwLock<(Ipv4Addr, u32)>>,
    pub ip_list: Arc<RwLock<Vec<(Ipv4Addr, u32)>>>,
    pub node_ip: Ipv4Addr,
    pub node_id: u32,
    pub port: u16,
    pub in_election: Arc<RwLock<bool>>,
}

#[cfg(feature = "ssr")]
impl AppState {
    pub fn new(leptos_options: LeptosOptions, node_ip: Ipv4Addr, port: u16, node_id: u32) -> Self {
        Self {
            leptos_options,
            leader: Arc::new(RwLock::new((node_ip, node_id))),
            ip_list: Arc::new(RwLock::new(Vec::new())),
            node_ip,
            node_id,
            port,
            in_election: Arc::new(RwLock::new(false)),
        }
    }
}

#[cfg(feature = "ssr")]
impl axum::extract::FromRef<AppState> for LeptosOptions {
    fn from_ref(state: &AppState) -> Self {
        state.leptos_options.clone()
    }
}
