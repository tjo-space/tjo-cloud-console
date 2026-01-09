use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
#[allow(unused)]
pub struct Postgresql {
    pub host: String,
    pub user: String,
    pub password: String,
    pub sslmode: String,
    pub database: String,
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
pub struct S3 {
    pub url: String,
    pub token: String,
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
pub struct Settings {
    pub s3: S3,
    pub postgresql: HashMap<String, Postgresql>,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let settings = Config::builder()
            .add_source(
                Environment::with_prefix("TJOCLOUD")
                    .separator("__")
                    .try_parsing(true),
            )
            .add_source(File::with_name("settings").required(false))
            .add_source(File::with_name("/etc/console/settings").required(false))
            .build()?;

        settings.try_deserialize()
    }

    pub fn postgresql(&self) -> &HashMap<String, Postgresql> {
        &self.postgresql
    }

    pub fn s3(&self) -> &S3 {
        &self.s3
    }
}
