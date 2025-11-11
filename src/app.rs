use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes},
    StaticSegment,
};
use std::cell::Cell;
use serde::{Serialize, Deserialize};
use std::net::Ipv4Addr;

// The frontend representation of appState, since that one is behind server feature
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeStatus {
    pub this_node_id: u32,
    pub this_node_ip: Ipv4Addr,
    pub current_leader: (Ipv4Addr, u32),
    pub all_nodes: Vec<(Ipv4Addr, u32)>,
}

#[server]
pub async fn get_node_status() -> Result<NodeStatus, ServerFnError> {
    use crate::state::AppState;
    let state: AppState = use_context::<AppState>()
        .ok_or_else(|| ServerFnError::new("app state not found"))?;

    tracing::info!("found state: {:?}", state);

    leptos::logging::log!("found state: {:?}", state);
    let leader_guard = state.leader.read().await;
    let ip_list_guard = state.ip_list.read().await;
    leptos::logging::log!("found leader: {:?}", leader_guard);
    leptos::logging::log!("ip list: {:?}", ip_list_guard);

    let status = NodeStatus {
        this_node_id: state.node_id,
        this_node_ip: state.node_ip,
        current_leader: leader_guard.clone(),
        all_nodes: ip_list_guard.clone(),
    };
    
    Ok(status)

}

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone() />
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context();

    view! {
        // injects a stylesheet into the document <head>
        // id=leptos means cargo-leptos will hot-reload this stylesheet
        <Stylesheet id="leptos" href="/pkg/fortune_cookies.css"/>

        // sets the document title
        <Title text="Welcome to Leptos"/>

        // content for this welcome page
        <Router>
            <main>
                <Routes fallback=|| "Page not found.".into_view()>
                    <Route path=StaticSegment("") view=HomePage/>
                </Routes>
            </main>
        </Router>
    }
}

async fn fetch_cookie(node_state: NodeStatus) -> Result<String, String> {
    println!("Trying to get cookie");

    let leader_url = node_state.current_leader.0.to_string().replacen("0.0.0.0", "localhost", 1);
    let url = format!("http://{}:3000/api/get_cookie", leader_url);
    leptos::logging::log!("trying to reach url: {}", url);
    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("Error occurred whilst getting response: {}", e))?;
    if response.status().is_success() {
        response.text()
            .await
            .map_err(|e| format!("Body error: {}", e))
    } else {
        Err("app state not found".to_string())
    }
}

/// Renders the home page of your application.
#[component]
fn HomePage() -> impl IntoView {
    let status_resource = LocalResource::new(move || get_node_status());
    
    let cookie_val = LocalResource::new(move || {
        // This will re-run whenever status_resource changes
        let status = status_resource.get();
        
        async move {
            match status {
                Some(Ok(node_status)) => fetch_cookie(node_status).await,
                Some(Err(e)) => Err(format!("Status error: {}", e).to_string()),
                None => Err("Waiting for status".to_string()),
            }
        }
}); 
    let refresh_cookie = Action::new(move |&()| {
        async move {
            cookie_val.refetch();
        }
    });


    let refresh_action = Action::new(move |&()| {
        async move {
            status_resource.refetch();
        }
    });
    
    view! {
        <h2>Fortune cookie selector: </h2>
        <button on:click=move |_| { refresh_cookie.dispatch(()); }>"get new cookie"</button>

        
        <Suspense
            fallback=move || view! { <p>"Loading cookie..."</p> }
        >
            {move || {
                // status_resource.get() returns an Option<Result<T, E>>
                cookie_val.get().map(|result| {
                    match result {
                        // Success!
                        Ok(cookie) => view! {
                            <div>
                                <p> your cookie: </p>
                                <p> {cookie} </p>
                            </div>
                        }.into_any(),

                        // Error
                        Err(e) => view! {
                            <div>
                                <p style="color: red;">"Error loading cookie: " {e.to_string()}</p>
                            </div>
                        }.into_any(),
                    }
                })
            }}
        </Suspense>
        <h2>Node Status</h2>
        <button on:click=move |_| { refresh_action.dispatch(()); }>
            "Refresh Status"
        </button>
        // Use <Suspense> to show a fallback while data is loading
        <Suspense
            fallback=move || view! { <p>"Loading status..."</p> }
        >
            {move || {
                // status_resource.get() returns an Option<Result<T, E>>
                status_resource.get().map(|result| {
                    match result {
                        // Success!
                        Ok(status) => view! {
                            <div>
                                <p><b>This Node:</b> {format!("{}:{}", status.this_node_ip, status.this_node_id)}</p>
                                <p><b>Current Leader:</b> {format!("{}:{}", status.current_leader.0, status.current_leader.1)}</p>
                                <h3>All Nodes:</h3>
                                <ul>
                                    {
                                        status.all_nodes.iter().map(|(ip, id)| {
                                            view! { <li>{format!("{}:{}", ip, id)}</li> }
                                        }).collect_view()
                                    }
                                </ul>
                            </div>
                        }.into_any(),

                        // Error
                        Err(e) => view! {
                            <div>
                                <p style="color: red;">"Error loading status: " {e.to_string()}</p>
                            </div>
                        }.into_any(),
                    }
                })
            }}
        </Suspense>
    }
}
