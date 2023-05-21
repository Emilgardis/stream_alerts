use once_cell::sync::Lazy;

pub struct MakeConsoleWriter;
use std::io::{self, Write};
use tracing_subscriber::fmt::MakeWriter;

impl<'a> MakeWriter<'a> for MakeConsoleWriter {
    type Writer = ConsoleWriter;

    fn make_writer(&'a self) -> Self::Writer {
        unimplemented!("use make_writer_for instead");
    }

    fn make_writer_for(&'a self, meta: &tracing::Metadata<'_>) -> Self::Writer {
        ConsoleWriter(*meta.level(), Vec::with_capacity(256))
    }
}

pub struct ConsoleWriter(tracing::Level, Vec<u8>);

impl io::Write for ConsoleWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.1.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        use gloo::console;
        use tracing::Level;

        let data = String::from_utf8(self.1.to_owned())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "data not UTF-8"))?;

        match self.0 {
            Level::TRACE => console::debug!(&data),
            Level::DEBUG => console::debug!(&data),
            Level::INFO => console::log!(&data),
            Level::WARN => console::warn!(&data),
            Level::ERROR => console::error!(&data),
        }

        Ok(())
    }
}

impl Drop for ConsoleWriter {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

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

#[cfg(feature = "ssr")]
pub fn install_utils() -> eyre::Result<()> {
    let _ = dotenvy::dotenv(); //ignore error
    install_tracing();
    install_eyre()?;
    Ok(())
}

#[cfg(feature = "ssr")]
fn install_eyre() -> eyre::Result<()> {
    let (panic_hook, eyre_hook) = color_eyre::config::HookBuilder::default().into_hooks();

    eyre_hook.install()?;

    std::panic::set_hook(Box::new(move |pi| {
        tracing::error!("{}", panic_hook.panic_report(pi));
    }));
    Ok(())
}

pub fn install_tracing() {
    use tracing_error::ErrorLayer;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    let fmt_layer = fmt::layer().with_target(true).compact();
    #[rustfmt::skip]
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("debug"))
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
