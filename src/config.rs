use log::error;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use twitch_api_rs::Config;

pub fn get_config(location: Option<PathBuf>) -> Option<Config> {
    if let Ok(file) = File::open(location.unwrap_or(PathBuf::from(crate::DEFAULT_CONFIG_LOCATION)))
    {
        let buf_reader = BufReader::new(file);
        match serde_json::from_reader(buf_reader) {
            Ok(json) => json,
            Err(e) => {
                error!("Could not parse configuration file:\n{:#?}", &e);
                None
            }
        }
    } else {
        None
    }
}

pub fn write_default(location: Option<PathBuf>) {
    let file = File::create(location.unwrap_or(PathBuf::from(crate::DEFAULT_CONFIG_LOCATION)))
        .expect("Could not create config file");

    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, &Config::default())
        .expect("Could not serialize default config into file");

    println!(
        "Created Basic Config file at {}, please fill in information",
        crate::DEFAULT_CONFIG_LOCATION
    );
}
