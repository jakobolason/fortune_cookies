use axum::{body::Body, extract::{FromRef, State}, http::Request, middleware::{self, Next}, response::{IntoResponse, Response}, routing::get, Json, Router
};
use reqwest::StatusCode;
use core::fmt;
use std::{
    collections::{HashMap, HashSet}, 
    env, 
    net::{IpAddr, Ipv4Addr, SocketAddr}, 
    sync::{Arc}, 
    time::{SystemTime, UNIX_EPOCH}
};
use leptos::{config::LeptosOptions, html::{address, U}};
// use uuid::Uuid;
// use tower_http::trace::{TraceLayer, ServerErrorsFailureClass};
// use tracing::{info, error, info_span, Span};
// use std::time::Duration;
use tokio::{sync::RwLock, time::{sleep, Duration, interval}, net::lookup_host};

fn generate_node_id() -> u32 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    (now % 101) as u32
}

#[derive(Clone)]
struct AppState {
    leptos_options: LeptosOptions,
    leader: Arc<RwLock<(Ipv4Addr, u32)>>,
    ip_list: Arc<RwLock<Vec<(Ipv4Addr, u32)>>>,
    node_ip: Ipv4Addr,
    node_id: u32,
    port: u16,
    in_election: Arc<RwLock<bool>>,
}
impl AppState {
    fn new(leptos_options: LeptosOptions, node_ip: Ipv4Addr, port: u16, node_id: u32) -> Self {
        Self {
            leptos_options,
            leader: Arc::new(RwLock::new((Ipv4Addr::new(0, 0, 0, 0),0))),
            ip_list: Arc::new(RwLock::new(Vec::new())),
            node_ip,
            node_id,
            port,
            in_election: Arc::new(RwLock::new(false)),
        }
    }
}
impl FromRef<AppState> for LeptosOptions {
    fn from_ref(state: &AppState) -> Self {
        state.leptos_options.clone()
    }
}


async fn middleware_check_is_leader(State(state): State<AppState>, request: Request<Body>, next: Next) -> Response {

    println!("request to: {}", request.uri());
    println!("is leader? {:?}", state.leader);
    let response = next.run(request).await;

    println!("response status: {}", response.status());
    response
}

async fn get_ids_from_peers(peers: impl Iterator<Item = SocketAddr>, port: u16) -> Vec<(Ipv4Addr, u32)> {
    let mut ip_vec: Vec<(Ipv4Addr, u32)> = Vec::new();
    for addr in peers {
        if let IpAddr::V4(ipv4) = addr.ip() {
            let url = format!("http://{}:{}/node_id", ipv4, port);
            match reqwest::get(&url).await {
                Ok(resp) => {
                    match resp.json::<u32>().await {
                        Ok(node_id) => {
                            println!("Got node_id {} from {:?}", node_id, ipv4);
                            ip_vec.push((ipv4, node_id));
                        }
                        Err(e) => eprintln!("Error parsing response from {}: {}", ipv4, e),
                    }
                }
                Err(e) => eprintln!("Error with request to {}: {}", ipv4, e),
            } 
        }    
    }
    ip_vec
}

async fn dns_lookup(service_name: &str, port: u16) -> Result<Vec<SocketAddr>, std::io::Error> {
    println!("Making a DNS lookup to service");
    
    let address = format!("{}:{}", service_name, port);
    let addresses = lookup_host(&address).await?;
    
    println!("Got response from DNS");
    Ok(addresses.collect())
}
async fn discovery(service_name: &str, port: u16) -> Result<Vec<(Ipv4Addr, u32)>, std::io::Error> {
    let peer_addrs = dns_lookup(service_name, port).await?;
    // now get the ids of all these ips
    let ip_vec = get_ids_from_peers(peer_addrs.into_iter(), port).await; 
    Ok(ip_vec)
}

// background heartbeat check
async fn background_task(state: AppState) {
    // let other nodes settle
    let mut interval = interval(Duration::from_secs(10));
    
    loop {
        interval.tick().await;
        println!("background task for node {} running at: {:?}",state.node_id, std::time::SystemTime::now());
        match discovery("bully-service", state.port).await {
            Ok(ips) => {
                println!("Found ips: {:?}", ips);
            }
            Err(e) => {
                eprintln!("DNS lookup failed: {}", e);
            }
        }
    }

}

async fn write_new_leader(state: &AppState, leader: (Ipv4Addr, u32)) {
    {
        let mut leader_guard = state.leader.write().await;
        *leader_guard = leader;
    }
}
#[derive(serde::Serialize, Debug)]
enum MessageEndpoints {
    Coordinator,
    Election,
}
impl MessageEndpoints {
    fn as_str(&self) -> &'static str {
        match self {
            MessageEndpoints::Coordinator => "recieve_coordinator",
            MessageEndpoints::Election => "recieve_election"
        }
    }
}
#[derive(serde::Serialize, Debug)]
struct MessageType {
    ip: Ipv4Addr,
    node_id: u32,
}

async fn send_unicast(port: u16, msg: MessageType, endpoint: MessageEndpoints) 
    -> Result<reqwest::Response, reqwest::Error> {
    let url = format!("http://{}:{}/{}", msg.ip, port, endpoint.as_str());
    return reqwest::get(&url).await
}

async fn send_coordinator(state: &AppState, ip_list: Vec<(Ipv4Addr, u32)>) 
    -> Vec<Result<reqwest::Response, reqwest::Error>> {
     
    let ips: Vec<Ipv4Addr> = ip_list.iter().map(|(ip, _)| ip.clone()).collect();
    let payload = MessageType {
        ip: state.node_ip, node_id: state.node_id
    };
    let client = reqwest::Client::new();
    let mut resp: Vec<Result<reqwest::Response, reqwest::Error>> = Vec::new(); 
    for ip in ips {
        let url = format!("http://{}:{}/{}", ip, state.port, MessageEndpoints::Coordinator.as_str());
        let res = client.post(url).json(&payload).send().await;
        resp.push(res);
    }
    resp
}

async fn send_election(endpoints: Vec<Ipv4Addr>, port: u16) 
    -> Vec<Result<reqwest::Response, reqwest::Error>> {
    let mut resp: Vec<Result<reqwest::Response, reqwest::Error>> = Vec::new(); 
    for ip in endpoints {
        let url = format!("http://{}:{}/{}", ip, port, MessageEndpoints::Election.as_str());
        resp.push(reqwest::get(&url).await);
    }
    resp
}


// election algorithmn
async fn leader_election(state: AppState) {
    println!("Starting election for node {}", state.node_id);
    let ip_list_guard = state.ip_list.read().await;
    let ip_list = ip_list_guard.clone();
    let election_candidates: Vec<Ipv4Addr> = ip_list
        .iter()
        .filter(|(_, id)| *id > state.node_id)
        .map(|(ip, _)| ip.clone())
        .collect();
    println!("Node {} will send elections to {} candidates", state.node_id, election_candidates.len());

    let is_leader = if election_candidates.len() != 0 {
        // else ask every candidate, and wait for 1 ok
        let responses = send_election(election_candidates, state.port).await;
        let ok_recieved = responses
            .iter()
            .any(|r| matches!(r, Ok(x) if x.status() == reqwest::StatusCode::OK));
        if ok_recieved {
            // then a higher indexed node is alive
            false
        } else {
            // elsewise there are none, and this is leader
            true
        }
    } else {
        true
    };
    if is_leader {
        // if no candidates or no candidates responded, we are leader 
        write_new_leader(&state, (state.node_ip.clone(), state.node_id.clone())).await;
        // send broadcast, saying it is now leader
        send_coordinator(&state, ip_list).await;

    }
    // finally release the lock
    {
        let mut election_guard = state.in_election.write().await;
        *election_guard = false;
    }
}

// endpoints
#[derive(serde::Deserialize, Debug)]
struct CoordinatorInfo {
    node_id: u32,
    ip: Ipv4Addr,
}
async fn recieve_coordinator(
    State(state): State<AppState>, 
    Json(payload): Json<CoordinatorInfo>
    ) -> impl IntoResponse {
    // get the ip and node_id from request
    println!("Got node_id = {} from IP = {}", payload.node_id, payload.ip);
    write_new_leader(&state, (payload.ip, payload.node_id)).await;
    (StatusCode::OK, "Recieved")
}
async fn recieve_election(State(state): State<AppState>) -> &'static str {
    {
        let mut election_guard = state.in_election.write().await;
        if !*election_guard {
            *election_guard = true;
            tokio::spawn(leader_election(state.clone()));
        }
    }
    "OK"
}

async fn get_status() -> &'static str{
    "OK"
}

async fn get_node_id(State(state): State<AppState>) -> Json<u32> {
    Json(state.node_id)
}

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use leptos::logging::log;
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use fortune_cookies::app::*;

    let port = env::var("WEB_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(3000);
    
    // Override the site_addr with environment variable
    let mut conf = get_configuration(None).unwrap();
    conf.leptos_options.site_addr = format!("0.0.0.0:{}", port).parse().unwrap();
    
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    // Generate the list of routes in your Leptos App
    let routes = generate_route_list(App);
    let ip_addr = match addr.ip() {
        IpAddr::V4(ipv4) => ipv4,
        IpAddr::V6(_) => panic!("IPv6 not supported!")
    };
    let state = AppState::new(leptos_options, ip_addr, port, generate_node_id());

    let app = Router::new()
        .leptos_routes(&state, routes, {
            let leptos_options = state.leptos_options.clone();
            move || shell(leptos_options.clone())
        })
        .route("/status",get(get_status))
        .route("/node_id", get(get_node_id))
        .route("/recieve_election", get(recieve_election))
        .route("/recieve_coordination", get(recieve_coordinator))
        // .layer(tracing_layer)
        .layer(middleware::from_fn_with_state(state.clone(), middleware_check_is_leader))
        .fallback(leptos_axum::file_and_error_handler::<AppState, _>(shell))
        .with_state(state.clone());

    let background_handle = tokio::spawn(background_task(state.clone()));
    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    log!("listening on http://{}", &addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
    background_handle.abort();
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    // no client-side main function
    // unless we want this to work with e.g., Trunk for pure client-side testing
    // see lib.rs for hydration function instead
}

#[cfg(test)]
mod tests {
    use super::*; // Import everything from the parent module (your main.rs)
    use tokio::net::TcpListener;
    use std::net::SocketAddr;
    use leptos::logging::log;
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use fortune_cookies::app::*;
    
    /// A helper function to spawn a test server.
    /// It binds to a random available port (127.0.0.1:0).
    async fn spawn_test_server(node_id: u32) -> (SocketAddr, AppState) {
        // Bind to port 0 to get a random available port from the OS
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let port = addr.port();

        let mut conf = get_configuration(None).unwrap();

        conf.leptos_options.site_addr = format!("0.0.0.0:{}", port).parse().unwrap();
        conf.leptos_options.output_name = "test".to_string().into();
        conf.leptos_options.site_addr = addr;
        let addr = conf.leptos_options.site_addr;
        let leptos_options = conf.leptos_options;

        // Create a test AppState
        let state = AppState::new(
            leptos_options.clone(), 
            u32::from(Ipv4Addr::new(127, 0, 0, 1)), 
            port, 
            node_id
        );

        // Build the router
        let app = Router::new()
            .route("/status", get(get_status))
            .route("/node_id", get(get_node_id))
            .with_state(state.clone());

        // Spawn the server in the background
        tokio::spawn(async move {
            axum::serve(listener, app.into_make_service())
                .await
                .unwrap();
        });

        (addr, state)
    }

    /// Test 1: Simple Endpoint Test for /status
    #[tokio::test]
    async fn test_status_endpoint() {
        let (addr, _state) = spawn_test_server(1).await;
        let client = reqwest::Client::new();

        let res = client
            .get(format!("http://{}/status", addr))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), reqwest::StatusCode::OK);
        assert_eq!(res.text().await.unwrap(), "OK");
    }

    /// Test 2: Simple Endpoint Test for /node_id
    #[tokio::test]
    async fn test_get_node_id_endpoint() {
        let test_node_id = 123;
        let (addr, _state) = spawn_test_server(test_node_id).await;
        let client = reqwest::Client::new();

        let res = client
            .get(format!("http://{}/node_id", addr))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), reqwest::StatusCode::OK);
        assert_eq!(res.json::<u32>().await.unwrap(), test_node_id);
    }

    // Test 3: Inter-communication Test (doesn't work for now)
    // #[tokio::test]
    // async fn test_peer_discovery_logic() {
    //     // 1. Spawn 3 "nodes" (test servers)
    //     let (addr1, state1) = spawn_test_server(101).await;
    //     let (addr2, state2) = spawn_test_server(102).await;
    //     let (addr3, state3) = spawn_test_server(103).await;
    //
    //     let peer_addrs = vec![addr1, addr2, addr3];
    //     let expected_node_ids = vec![
    //         state1.node_id, 
    //         state2.node_id, 
    //         state3.node_id
    //     ];
    //
    //     // 2. Call our testable function `get_ids_from_peers`
    //     // We use state1.port, but they should all be the same in this test.
    //     let discovered_peers = get_ids_from_peers(
    //         peer_addrs.into_iter(), 
    //         state1.port
    //     ).await;
    //
    //     // 3. Assert the results
    //     assert_eq!(discovered_peers.len(), 3);
    //
    //     let mut found_ids: Vec<u32> = discovered_peers
    //         .into_iter()
    //         .map(|(_ip, id)| id)
    //         .collect();
    //
    //     let mut expected_ids = expected_node_ids;
    //
    //     // Sort both lists to compare them
    //     found_ids.sort();
    //     expected_ids.sort();
    //
    //     assert_eq!(found_ids, expected_ids);
    //     println!("Successfully discovered peers: {:?}", found_ids);
    // }
}
