use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[clap(about, version, long_version = &**crate::util::LONG_VERSION )]
pub struct Opts {
    #[clap(long, env, hide_env = true)]
    pub db_path: PathBuf,
    #[clap(long, env, hide_env = true)]
    pub admin_password: Secret,
}

#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    pub fn secret(&self) -> &str {
        &self.0
    }
}

impl std::str::FromStr for Secret {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[secret]")
    }
}
