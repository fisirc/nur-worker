use lazy_static::lazy_static;
use std::env;

macro_rules! env_var {
    ($name:expr) => {
        env::var($name).expect(&format!("{} must be set", $name))
    };
}

macro_rules! env_var_or {
    ($name:expr, $default:expr) => {
        env::var($name).unwrap_or($default.into())
    };
}

lazy_static! {
    /// The port where the worker will be listening
    pub static ref PORT: u16 = env_var_or!("PORT", "6969")
        .parse::<u16>().expect("PORT must be a number");

    /// The host where the worker will be listening
    pub static ref HOST: String = env_var_or!("HOST", "0.0.0.0");

    pub static ref S3_ACCESS_KEY_ID: String = env_var!("S3_ACCESS_KEY_ID");

    pub static ref S3_SECRET_ACCESS_KEY: String = env_var!("S3_SECRET_ACCESS_KEY");

    pub static ref S3_REGION: String = env_var_or!("S3_REGION", "us-east-2");

    pub static ref CACHE_DIR: String = env_var_or!("CACHE_DIR", ".cache");

    pub static ref CARGO_PKG_NAME: String = env_var!("CARGO_PKG_NAME");

    pub static ref POSTGRES_URL: String = env_var!("POSTGRES_URL");
}
