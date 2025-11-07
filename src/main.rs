use axum::{Router, 
    routing::get,
    middleware::{self, Next},
    response::Response,
    http::Request,
    body::Body,
    extract::{FromRef, State},
    Json,
};
use std::{
    net::{IpAddr, Ipv4Addr}, 
    sync::{Arc, RwLock}, 
    time::{SystemTime, UNIX_EPOCH},
    collections::{HashSet, HashMap},
    env,
};
use leptos::{config::LeptosOptions, html::U};
// use uuid::Uuid;
// use tower_http::trace::{TraceLayer, ServerErrorsFailureClass};
// use tracing::{info, error, info_span, Span};
// use std::time::Duration;
use tokio::{time::{sleep, Duration, interval}, net::lookup_host};

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
    leader: Arc<RwLock<(u32, u32)>>,
    ip_list: Arc<RwLock<Vec<(u32, u32)>>>,
    node_ip: u32,
    node_id: u32,
    port: u16,
}
impl AppState {
    fn new(leptos_options: LeptosOptions, node_ip: u32, port: u16, node_id: u32) -> Self {
        Self {
            leptos_options,
            leader: Arc::new(RwLock::new((0,0))),
            ip_list: Arc::new(RwLock::new(Vec::new())),
            node_ip,
            node_id,
            port,
        }
    }
}
impl FromRef<AppState> for LeptosOptions {
    fn from_ref(state: &AppState) -> Self {
        state.leptos_options.clone()
    }
}


async fn middleware_check_is_leader(request: Request<Body>, next: Next) -> Response {
    println!("request to: {}", request.uri());
    let response = next.run(request).await;

    println!("response status: {}", response.status());
    response
}

async fn discovery(service_name: &str, port: u16) -> Result<Vec<(u32, u32)>, std::io::Error> {
    println!("Making a DNS lookup to service");
    
    let address = format!("{}:{}", service_name, port);
    let addresses = lookup_host(&address).await?;
    
    println!("Got response from DNS");
    
    let mut ip_list: Vec<Ipv4Addr> = Vec::new();
    for addr in addresses {
        if let std::net::IpAddr::V4(ipv4) = addr.ip() {
            ip_list.push(ipv4);
        }
    }
    
    // now get the ids of all these ips
    let mut ip_map = HashMap::new();
    for ip in ip_list {
        let url = format!("http://{}:{}/node_id", ip, port);
        let ip_addr = u32::from(ip);
        match reqwest::get(&url).await {
            Ok(resp) => {
                match resp.json::<u32>().await {
                    Ok(node_id) => {
                        println!("Got node_id {} from {:?}", node_id, ip_addr);
                        ip_map.insert(ip_addr, node_id);
                    }
                    Err(e) => eprintln!("Error parsing response from {}: {}", ip_addr, e),
                }
            }
            Err(e) => eprintln!("Error with request to {}: {}", ip_addr, e),
        } 
    }
    let ip_vec: Vec<(u32, u32)> = ip_map.into_iter().collect();
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
        IpAddr::V4(ipv4) => u32::from(ipv4),
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
        // .layer(tracing_layer)
        .layer(middleware::from_fn(middleware_check_is_leader))
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

async fn get_status() -> &'static str{
    "OK"
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    // no client-side main function
    // unless we want this to work with e.g., Trunk for pure client-side testing
    // see lib.rs for hydration function instead
}
