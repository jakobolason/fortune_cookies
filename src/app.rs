use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes},
    StaticSegment,
};
use std::cell::Cell;

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

/// Renders the home page of your application.
#[component]
fn HomePage() -> impl IntoView {
    let cookies = vec![
        "Today its up to you to create the peacefulness you long for.",
        "A friend asks only for your time not your money.",
        "If you refuse to accept anything but the best, you very often get it.",
        "A smile is your passport into the hearts of others.",
        "A good way to keep healthy is to eat more Chinese food.",
        "Your high-minded principles spell success.",
        "Hard work pays off in the future, laziness pays off now.",
        "Change can hurt, but it leads a path to something better.",
        "Enjoy the good luck a companion brings you.",
        "People are naturally attracted to you.",
        "Hidden in a valley beside an open stream- This will be the type of place where you will find your dream.",
        "A chance meeting opens new doors to success and friendship.",
        "You learn from your mistakes... You will learn a lot today.",
        "If you have something good in your life, don’t let it go!",
        "What ever you’re goal is in life, embrace it visualize it, and for it will be yours.",
        "Your shoes will make you happy today.",
        "You cannot love life until you live the life you love.",
        "Be on the lookout for coming events; They cast their shadows beforehand.",
        "Land is always on the mind of a flying bird.",
        "The man or woman you desire feels the same about you.",
        "Meeting adversity well is the source of your strength.",
        "A dream you have will come true.",
        "Our deeds determine us, as much as we determine our deeds.",
        "Never give up. You’re not a failure if you don’t give up.",
        "You will become great if you believe in yourself.",
        "There is no greater pleasure than seeing your loved ones prosper.",
        "You will marry your lover.",
        "A very attractive person has a message for you.",
        "You already know the answer to the questions lingering inside your head.",
        "It is now, and in this world, that we must live.",
        "You must try, or hate yourself for not trying.",
        "You can make your own happiness."
    ];
    let cookie_val = RwSignal::new(cookies[0]);
    let seed = Cell::new(12345u32);

    let new_cookie_click = move |_| *cookie_val.write() = {
        let current = seed.get();
        let next = current.wrapping_mul(1103515245).wrapping_add(12345);
        seed.set(next);

        let random_index = (next as usize) % cookies.len();
        cookies[random_index]
    };
    
    view! {
        <h2>Fortune cookie selector: </h2>
        <button on:click=new_cookie_click>"get new cookie"</button>
        <p>
            {cookie_val}
        </p> 
    }
}
