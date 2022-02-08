use clap::{ArgSettings, Parser};

#[derive(Parser, Debug, Clone)]
#[clap(about, version, long_version = &**crate::util::LONG_VERSION )]
pub struct Opts {
    #[clap(long, env, setting = ArgSettings::HideEnvValues, parse(from_str))]
    pub client_id: twitch_api2::twitch_oauth2::ClientId,
    #[clap(long, env, setting = ArgSettings::HideEnvValues, parse(from_str))]
    pub client_secret: twitch_api2::twitch_oauth2::ClientSecret,
    #[clap(long, env, setting = ArgSettings::HideEnvValues)]
    pub sign_secret: SignSecret,
    #[clap(long, env, setting = ArgSettings::HideEnvValues, parse(from_str))]
    pub broadcaster_login: twitch_api2::types::UserName,
    #[clap(long, env, setting = ArgSettings::HideEnvValues)]
    pub website_callback: String,
}

#[derive(Clone)]
pub struct SignSecret {
    secret: String,
}

impl SignSecret {
    /// Get a reference to the sign secret.
    pub fn secret(&self) -> &[u8] { self.secret.as_bytes() }

    pub fn secret_str(&self) -> &str { &self.secret }
}

impl std::fmt::Debug for SignSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignSecret")
            .field("secret", &"[redacted]")
            .finish()
    }
}

impl std::str::FromStr for SignSecret {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(SignSecret {
            secret: s.to_string(),
        })
    }
}
