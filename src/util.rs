use eyre::Context;
use once_cell::sync::Lazy;

pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

pub static LONG_VERSION: Lazy<String> = Lazy::new(|| {
    let version = if let Some(hash) = built_info::GIT_COMMIT_HASH {
        if let Some(true) = built_info::GIT_DIRTY {
            format!("{} ({}*)", built_info::PKG_VERSION, hash)
        } else {
            format!("{} ({})", built_info::PKG_VERSION, hash)
        }
    } else {
        built_info::PKG_VERSION.to_string()
    };
    format!(
        "{version}\nbuilt with {}\nbuild timestamp: {}",
        built_info::RUSTC_VERSION,
        built_info::BUILT_TIME_UTC
    )
});

pub fn install_utils() -> eyre::Result<()> {
    let _ = dotenv::dotenv(); //ignore error
    install_tracing();
    install_eyre()?;
    Ok(())
}

fn install_eyre() -> eyre::Result<()> {
    let (panic_hook, eyre_hook) = color_eyre::config::HookBuilder::default().into_hooks();

    eyre_hook.install()?;

    std::panic::set_hook(Box::new(move |pi| {
        tracing::error!("{}", panic_hook.panic_report(pi));
    }));
    Ok(())
}

fn install_tracing() {
    use tracing_error::ErrorLayer;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    let fmt_layer = fmt::layer().with_target(true);
    #[rustfmt::skip]
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .map(|f| {
            f.add_directive("hyper=error".parse().expect("could not make directive"))
                .add_directive("h2=error".parse().expect("could not make directive"))
                .add_directive("rustls=error".parse().expect("could not make directive"))
                .add_directive("tungstenite=error".parse().expect("could not make directive"))
                .add_directive("retainer=info".parse().expect("could not make directive"))
            //.add_directive("tower_http=error".parse().unwrap())
        })
        .expect("could not make filter layer");

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .with(ErrorLayer::default())
        .init();
}
