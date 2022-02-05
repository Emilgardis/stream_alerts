use clap::{Parser, ArgSettings};

#[derive(Parser, Debug)]
#[clap(about, version, long_version = &**crate::util::LONG_VERSION )]
pub struct Opts {
    #[clap(long, env, setting = ArgSettings::HideEnvValues, parse(from_str))]
    pub client_id: twitch_api2::twitch_oauth2::ClientId,
    #[clap(long, env, setting = ArgSettings::HideEnvValues, parse(from_str))]
    pub client_secret: twitch_api2::twitch_oauth2::ClientSecret,
}