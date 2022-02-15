use std::path::PathBuf;

use clap::{ArgSettings, Parser};

#[derive(Parser, Debug, Clone)]
#[clap(about, version, long_version = &**crate::util::LONG_VERSION )]
pub struct Opts {
    #[clap(long, env, setting = ArgSettings::HideEnvValues)]
    pub db_path: PathBuf,
}
