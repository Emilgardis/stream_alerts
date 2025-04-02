use cfg_if::cfg_if;
pub mod alerts;
pub mod app;
pub mod error_template;
pub mod fileserv;
pub mod opts;
pub mod util;

#[cfg(feature = "ssr")]
pub mod ip;

pub mod auth;

cfg_if! { if #[cfg(feature = "hydrate")] {
    use leptos::*;
    use wasm_bindgen::prelude::wasm_bindgen;
    use crate::app::*;
    use tracing_subscriber::util::SubscriberInitExt;

    #[wasm_bindgen]
    pub fn hydrate() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .without_time()
            .with_file(true)
            .with_line_number(true)
            .with_target(false)
            .with_writer(util::MakeConsoleWriter)
            .with_ansi(false)
            .pretty()
            .finish()
            .init();

        console_error_panic_hook::set_once();

        leptos::mount::hydrate_body(App);
    }
}}
