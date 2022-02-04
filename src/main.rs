pub mod opts;
pub mod util;

use opts::Opts;

use clap::Parser;
use eyre::Context;


#[tokio::main]
async fn main() -> Result<(), color_eyre::Report> {
    let _ = util::install_utils()?;
    let opts = Opts::parse();

    tracing::debug!(
        "App started!\n{}",
        Opts::try_parse_from(&["app", "--version"])
            .unwrap_err()
            .to_string()
    );

    run(&opts).await.with_context(|| "When running application")?;

    Ok(())
}

pub async fn run(_opts: &Opts) -> color_eyre::Result<()> {
    todo!()
}
