use clap::Parser;

#[derive(Parser, Debug)]
#[clap(about, version, long_version = &**crate::util::LONG_VERSION )]
pub struct Opts {

}