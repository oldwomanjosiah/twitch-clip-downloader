use log::{info, trace};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(tag = "type")]
pub enum Pagination {
    Forwards(String),
    Backwards(String),
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct State {
    pub auth_token: Option<String>,
    pub auth_timeout: Option<time::OffsetDateTime>,
    pub pagination: Option<Pagination>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            auth_token: None,
            auth_timeout: None,
            pagination: None,
        }
    }
}

pub fn load(path: Option<PathBuf>) -> Option<State> {
    trace!("Trying to read state file");
    if let Ok(file) = File::open(path.unwrap_or(PathBuf::from(crate::DEFAULT_STATE_LOCATION))) {
        trace!("File opened");
        let buf_reader = BufReader::new(file);

        match serde_json::from_reader(buf_reader) {
            Ok(read) => Some(read),
            Err(_) => {
                info!("Could not parse state file");
                None
            }
        }
    } else {
        info!("Could not open state file, may not exist yet");
        None
    }
}

pub fn save(state: &State, path: Option<PathBuf>) {
    trace!("Attempting to save state file");

    let file = File::create(path.unwrap_or(PathBuf::from(crate::DEFAULT_STATE_LOCATION)))
        .expect("Could not write to State file");

    let buf_writer = BufWriter::new(file);

    serde_json::to_writer_pretty(buf_writer, state).expect("Could not serialize state");
}
