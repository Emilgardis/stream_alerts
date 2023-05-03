use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[clap(about, version, long_version = &**crate::util::LONG_VERSION )]
pub struct Opts {
    #[clap(long, env, hide_env = true)]
    pub db_path: PathBuf,
    #[clap(long, short, default_value = "80")]
    pub port: u16,
    #[clap(long, short, default_value = "127.0.0.1")]
    pub interface: std::net::IpAddr,
}
