use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[clap(about, version, long_version = &**crate::util::LONG_VERSION )]
pub struct Opts {
    #[clap(long, env, hide_env = true)]
    pub db_path: PathBuf,
}
