use clap::{ArgSettings, Parser};

#[derive(Parser, Debug)]
#[clap(about, version, long_version = &**crate::util::LONG_VERSION )]
pub struct Opts {
    #[clap(long, env, setting = ArgSettings::HideEnvValues, parse(from_str))]
    pub client_id: twitch_api2::twitch_oauth2::ClientId,
    #[clap(long, env, setting = ArgSettings::HideEnvValues, parse(from_str))]
    pub client_secret: twitch_api2::twitch_oauth2::ClientSecret,
    #[clap(long, env, setting = ArgSettings::HideEnvValues)]
    pub sign_secret: crate::SignSecret,
    #[clap(long, env, setting = ArgSettings::HideEnvValues, parse(from_str))]
    pub broadcaster_id: twitch_api2::types::UserId,
}
