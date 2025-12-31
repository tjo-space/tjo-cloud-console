use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[allow(unused)]
struct Postgresql {
    name: String,
    address: String,
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
struct S3 {
    address: String,
    token: String,
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
pub struct Settings {
    s3: S3,
    postgresql: Vec<Postgresql>,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let settings = Config::builder()
            .add_source(Environment::with_prefix("TJOCLOUD"))
            .add_source(File::with_name("settings").required(false))
            .build()?;

        settings.try_deserialize()
    }
}
