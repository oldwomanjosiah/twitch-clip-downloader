mod args;
mod clip_download;
mod config;
mod state;

use reqwest::Client;
use reqwest::{
    self,
    header::{HeaderMap, HeaderValue},
};
use state::*;
use structopt::StructOpt;

use log::{debug, error, info, trace, warn};
use rayon::prelude::*;
use regex::Regex;
use std::path::PathBuf;
use time::prelude::*;
use twitch_api_rs::request::application_auth::*;

const USER_AGENT: &'static str = "TWITCH_CLIP_DOWNLOADER/0.1";
const DEFAULT_CONFIG_LOCATION: &'static str = "config.json";
const DEFAULT_STATE_LOCATION: &'static str = "state.json";
const DEFAULT_CLIP_INFO_LOCATION: &'static str = "clip_info/";
const DEFAULT_DOWNLOAD_LOCATION: &'static str = "clips/";

fn create_client_with_headers() -> Client {
    info!("Creating Client.");
    Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .expect("Could not build client")
}

fn create_request_auth_headers(config: &Config, state: &State) -> HeaderMap {
    info!("Creating request Headers.");
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        HeaderValue::from_str(&format!(
            "Bearer {}",
            state
                .auth_token
                .clone()
                .expect("Could not get access token for requests")
        ))
        .expect("Could not create auth header value"),
    );
    headers.insert(
        "client-id",
        HeaderValue::from_str(&config.client_id).expect("Could not insert client-id header value"),
    );
    headers
}

use twitch_api_rs::Config;

pub async fn update_auth(
    config: &Config,
    state: &mut crate::state::State,
    state_path: Option<PathBuf>,
) {
    trace!("Checking on auth status");
    if let Some(auth_timeout) = state.auth_timeout {
        let now = time::OffsetDateTime::now_utc();
        if auth_timeout > now {
            trace!("Reusing Auth Token");
            let mut remaining = auth_timeout - now;
            info!(
                "Time before re-auth: Days: {} HMS: {}-{}-{}",
                remaining.whole_days(),
                {
                    remaining -= remaining.whole_days().days();
                    remaining.whole_hours()
                },
                {
                    remaining -= remaining.whole_hours().hours();
                    remaining.whole_minutes()
                },
                {
                    remaining -= remaining.whole_minutes().minutes();
                    remaining.whole_seconds()
                }
            );
            return; // Return early if the timeout has not yet passed
        }
    }

    info!("Getting new Auth token for account");
    let auth_request: AuthRequest = AuthRequestBuilder::default()
        .client_id(config.client_id.clone())
        .client_secret(config.client_secret.clone())
        .build()
        .expect("Could not create auth request");

    let auth_response: AuthResponse = match auth_request.make_request(&Client::new()).await {
        Some(PossibleAuthResponse::AuthResponse(auth_response)) => auth_response,
        Some(PossibleAuthResponse::BadRequest(bad_request)) => {
            warn!("Bad Request: {:#?}", bad_request);
            std::process::exit(-1);
        }
        None => {
            warn!("Could not complete request");
            std::process::exit(-1);
        }
    };

    info!("Recieved Auth Response:\n{:#?}", &auth_response);
    state.auth_token.replace(auth_response.access_token);
    state
        .auth_timeout
        .replace(time::OffsetDateTime::now_utc() + auth_response.expires_in.seconds());

    info!("Auth changed, writing into state");
    crate::state::save(state, state_path);
}

pub async fn get_clip_info(
    _user: String,
    client: &Client,
    spinner_style: indicatif::ProgressStyle,
    headers: HeaderMap,
) -> clip_download::Clips {
    let bar = indicatif::ProgressBar::new_spinner().with_style(spinner_style);
    bar.set_message("Retrieving Clips");
    bar.enable_steady_tick(50);

    let ret =
        clip_download::get_all_clip_info(String::from("nikeairjordan9"), &client, headers).await;
    if let Some(inner) = ret {
        bar.finish_with_message(&format!("Finished with {} items", inner.clips.len()));
        inner
    } else {
        bar.finish_with_message("Failed to return data");
        error!("Could not get clips data");
        std::process::exit(-1);
    }
}

fn create_download_links(
    mut clips: clip_download::Clips,
    bar_style: indicatif::ProgressStyle,
) -> clip_download::Clips {
    let regex = Regex::new(r"(?m)(-preview-\d+x\d+\.[a-zA-Z]+)").expect("Could not create regex");
    let replace = r".mp4";

    let bar = indicatif::ProgressBar::new(clips.clips.len() as u64).with_style(bar_style);
    bar.set_message("Creating download urls");
    bar.enable_steady_tick(50);

    (clips.clips)
        .par_iter_mut()
        .for_each(|clip: &mut clip_download::ClipInfo| {
            clip.video_url = Some(regex.replace(&clip.thumbnail_url, replace).to_string());
            bar.inc(1);
        });

    bar.finish_with_message("Finished creating video urls");
    clips
}

async fn download_clips(
    client: Client,
    clips: clip_download::Clips,
    user: &String,
    location: Option<PathBuf>,
    bar_style: indicatif::ProgressStyle,
) {
    let location = if let Some(loc) = location {
        loc
    } else {
        let mut loc = PathBuf::from(DEFAULT_DOWNLOAD_LOCATION);
        loc.push(user);
        loc
    };

    std::fs::DirBuilder::new()
        .recursive(true)
        .create(&location)
        .expect("Could not create download dir");

    let bar = indicatif::ProgressBar::new(clips.clips.len() as u64).with_style(bar_style);
    bar.set_message("Dowloading Clips");
    bar.tick();
    bar.enable_steady_tick(50);

    let regex = Regex::new(r"-offset-(\d+)").expect("Could not compile regex");
    let osify = Regex::new(r"(?m)(/)").expect("Could not build regex");

    let mut infos = Vec::with_capacity(clips.clips.len());
    let mut returns = Vec::with_capacity(clips.clips.len());
    ((clips.clips)
        .into_par_iter()
        .map(|clip: clip_download::ClipInfo| {
            if let Some(clip_url) = clip.video_url {
                let mut loc = location.clone();
                loc.push(&format!(
                    "{}({}) {}.mp4",
                    &clip.created_date,
                    match regex.captures(&clip_url) {
                        Some(caps) => {
                            if let Some(cap) = caps.get(1) {
                                cap.as_str()
                            } else {
                                "0"
                            }
                        }
                        None => "0",
                    },
                    &osify.replace_all(&clip.name, r"-"),
                ));
                Some((clip_url, loc))
            } else {
                None
            }
        })
        .collect_into_vec(&mut infos));

    loop {
        // Had to switch to this method (only 10 * 2 file descriptors open at a time) because requests were timing out
        let mut join_handles = Vec::with_capacity(10);
        // Take the first 10 (or remaining if less), and set aside the remaining (or none if less)
        let new_inner = if infos.len() > 10 {
            infos.split_off(10)
        } else if infos.len() > 0 {
            Vec::new()
        } else {
            break;
        };

        // spawn a task for each
        for info in infos {
            if let Some((clip_url, clip_loc)) = info {
                join_handles.push(tokio::task::spawn(clip_download::download_clip(
                    client.clone(),
                    clip_url,
                    clip_loc,
                    bar.clone(),
                )));
            }
        }

        // Wait for each to finish
        for handle in join_handles.into_iter() {
            returns.push(handle.await);
        }

        infos = new_inner;
    }

    for ret in returns {
        if let Err(e) = ret {
            eprintln!("Could not download clip for reason: {}", e);
        }
    }

    bar.finish_with_message("Downloaded all clips");
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init(); // Fucks with progress bars

    trace!("Reading args");
    let args = args::Args::from_args();
    debug!("Parsed Args:\n{:#?}", &args);

    // Get configuration, exits application if initalized to default
    let config = match config::get_config(args.config.clone()) {
        Some(cfg) => cfg,
        None => {
            config::write_default(args.config);
            std::process::exit(-1);
        }
    };

    let mut state = match state::load(args.state.clone()) {
        Some(state) => state,
        None => state::State::default(),
    };

    let client = create_client_with_headers();

    let spinner_style = indicatif::ProgressStyle::default_spinner()
        .tick_chars("/|\\—")
        .template("| {spinner} | {wide_msg:.cyan} |");
    let bar_style = indicatif::ProgressStyle::default_bar()
        .progress_chars("=> ")
        .tick_chars("/|\\—")
        .template(
            "| {spinner} | {msg:40.cyan/white} [{wide_bar:.green}] [ Eta:{eta:4} | {pos:>4}/{len:4} ] |",
        );

    use args::Commands::*;
    match args.command {
        CheckAuth => {
            info!("Subcommand Auth");
            // Get the current auth token or if outdated then get a new one
            update_auth(&config, &mut state, args.state.clone()).await;
        }
        ClipInfo { user, clips } => {
            info!("Subcommand Get Clip Info");
            // Get the current auth token or if outdated then get a new one
            update_auth(&config, &mut state, args.state.clone()).await;

            state::save(&state, args.state.clone());

            let request_auth_headers = create_request_auth_headers(&config, &state);

            let resp = get_clip_info(
                user.clone(),
                &client,
                spinner_style.clone(),
                request_auth_headers.clone(),
            )
            .await;

            // Save to file
            let path = if let Some(path) = clips {
                path
            } else {
                // Ensure default path exists
                std::fs::DirBuilder::new()
                    .recursive(true)
                    .create(DEFAULT_CLIP_INFO_LOCATION)
                    .expect("Could not create default dir");

                let mut path = PathBuf::from(DEFAULT_CLIP_INFO_LOCATION);
                path.push(user);
                path.with_extension("json")
            };

            if let Ok(file) = std::fs::File::create(&path) {
                let writer = std::io::BufWriter::new(file);
                serde_json::to_writer_pretty(writer, &resp).expect("Could not serialize resp");
            } else {
                error!(
                    "Could not write to {}",
                    &path.to_str().expect("invalid path string")
                );
            }
        }
        DownloadLinks { user, clips } => {
            let path = if let Some(path) = clips {
                path
            } else if let Some(ref user) = user {
                let mut path = PathBuf::from(DEFAULT_CLIP_INFO_LOCATION);
                path.push(user);
                path.with_extension("json")
            } else {
                error!("Must provide either path or user");
                std::process::exit(-1);
            };

            let clips = if let Ok(file) = std::fs::File::open(&path) {
                let reader = std::io::BufReader::new(file);
                if let Ok(clips) = serde_json::from_reader::<_, clip_download::Clips>(reader) {
                    clips
                } else {
                    error!("Clip Info File was not able to be parsed");
                    std::process::exit(-1);
                }
            } else {
                if let Some(ref user) = user {
                    update_auth(&config, &mut state, args.state.clone()).await;

                    state::save(&state, args.state.clone());

                    let request_auth_headers = create_request_auth_headers(&config, &state);

                    get_clip_info(
                        user.clone(),
                        &client,
                        spinner_style.clone(),
                        request_auth_headers,
                    )
                    .await
                } else {
                    error!(
                        "Clip Info File does not exist and user not provided: {:?}",
                        &path
                    );
                    std::process::exit(-1);
                }
            };

            // create downlaod links
            let new_clips = create_download_links(clips, bar_style.clone());

            if let Ok(file) = std::fs::File::create(&path) {
                let writer = std::io::BufWriter::new(file);
                serde_json::to_writer_pretty(writer, &new_clips).expect("Could not serialize resp");
            } else {
                error!(
                    "Could not write to {}",
                    &path.to_str().expect("invalid path string")
                );
            }
        }
        DownloadClips { user, clips } => {
            info!("Subcommand Download Clips");
            let path = if let Some(path) = clips {
                path
            } else if let Some(ref user) = user {
                let mut path = PathBuf::from(DEFAULT_CLIP_INFO_LOCATION);
                path.push(user);
                path.with_extension("json")
            } else {
                error!("Must provide either path or user");
                std::process::exit(-1);
            };

            let mut clips = if let Ok(file) = std::fs::File::open(&path) {
                let reader = std::io::BufReader::new(file);
                if let Ok(clips) = serde_json::from_reader::<_, clip_download::Clips>(reader) {
                    clips
                } else {
                    error!("Clip Info File was not able to be parsed");
                    std::process::exit(-1);
                }
            } else {
                if let Some(ref user) = user {
                    update_auth(&config, &mut state, args.state.clone()).await;

                    state::save(&state, args.state.clone());

                    let request_auth_headers = create_request_auth_headers(&config, &state);

                    get_clip_info(
                        user.clone(),
                        &client,
                        spinner_style.clone(),
                        request_auth_headers,
                    )
                    .await
                } else {
                    error!(
                        "Clip Info File does not exist and user not provided: {:?}",
                        &path
                    );
                    std::process::exit(-1);
                }
            };

            if (clips.clips)
                .par_iter()
                .map(|clip: &clip_download::ClipInfo| clip.video_url.is_none())
                .reduce(|| true, |a, b| a & b)
            {
                info!("No clip contained dowload info so attempting to create download links");
                clips = create_download_links(clips, bar_style.clone());
            }

            info!("Downloading clips");
            download_clips(
                client.clone(),
                clips,
                &user.unwrap_or(String::from("empty")),
                None,
                bar_style.clone(),
            )
            .await;
        }
    }
    trace!("Finished");
}
