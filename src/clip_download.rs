

use log::*;
use reqwest::{header::HeaderMap, Client};
use serde::{Deserialize, Serialize};
use std::io::{Write};
use std::path::PathBuf;
use tokio::prelude::*;
use twitch_api_rs::request::get_clips::ClipsResponseItem;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ClipInfo {
    pub name: String,
    pub created_by: String,
    pub created_date: String,
    pub thumbnail_url: String,
    pub video_url: Option<String>,
}

unsafe impl Send for ClipInfo {}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Clips {
    pub clips: Vec<ClipInfo>,
}

impl Default for Clips {
    fn default() -> Self {
        Clips { clips: Vec::new() }
    }
}

impl Clips {
    pub fn with_capacity(capacity: usize) -> Self {
        Clips {
            clips: Vec::with_capacity(capacity),
        }
    }

    pub fn append_from_data(&mut self, data: Vec<ClipsResponseItem>) {
        for item in data {
            self.clips.push(ClipInfo {
                name: item.title,
                created_by: item.creator_name,
                created_date: item.created_at,
                thumbnail_url: item.thumbnail_url,
                video_url: None,
            });
        }
    }
}

pub async fn get_all_clip_info(user: String, client: &Client, headers: HeaderMap) -> Option<Clips> {
    let mut clip_info: Clips = Clips::default();

    // Get user id
    let user_id = {
        use twitch_api_rs::request::get_channel_information::*;

        match {
            ChannelInformationRequest::builder()
                .channel_name(user.clone())
                .build()
                .expect("Invariant failed")
                .make_request(client, headers.clone())
                .await
        } {
            Some(PossibleChannelInformationResponse::ChannelInformationResponse(channel_info)) => {
                if channel_info.data.len() < 1 {
                    error!("No user found by that name");
                    return None;
                } else if channel_info.data.len() > 1 {
                    warn!("More than one user by that name, assuming first");
                }
                channel_info.data[0].id.clone()
            }
            Some(PossibleChannelInformationResponse::BadRequest(err)) => {
                error!("Invalid auth:\n{:#?}", err);
                return None;
            }
            None => {
                error!("Invalid request");
                return None;
            }
        } // Return from match with userid
    };

    // Get all clips
    let mut pagination: Option<String> = Some(String::from(""));
    use twitch_api_rs::request::get_clips::*;
    loop {
        info!("Making request with key {:#?}", &pagination);
        match {
            ClipsRequest::builder()
                .broadcaster_id(user_id.clone())
                .after(pagination.take())
                .first(Some(20))
                .build()
                .expect("Could not build request")
                .make_request(&client, headers.clone())
                .await
        } {
            Some(PossibleClipsResponse::ClipsResponse(resp)) => {
                if resp.data.len() == 0 {
                    return Some(clip_info);
                }
                clip_info.append_from_data(resp.data);

                if let Some(pag) = resp.pagination.cursor {
                    pagination = Some(pag);
                } else {
                    return Some(clip_info);
                }
            }
            Some(PossibleClipsResponse::BadRequest(err)) => {
                error!("Invalid auth:\n{:#?}", err);
                return Some(clip_info);
            }
            None => {
                error!("Invalid request\n\n");
                return Some(clip_info);
            }
        };
    }
}

pub async fn download_clip(
    client: reqwest::Client,
    url: String,
    path: PathBuf,
    bar: indicatif::ProgressBar,
) -> Result<(), &'static str> {
    // get response
    let res = if let Ok(mut res) = client.get(&url).send().await {
        // Try to open file
        if let Ok(file) = tokio::fs::File::create(&path).await {
            let mut writer = tokio::io::BufWriter::new(file);
            // Write all bytes to file
            loop {
                match res.chunk().await {
                    Ok(Some(bytes)) => {
                        writer.write_all(&bytes).await;
                    }
                    Ok(None) => {
                        writer.flush().await;
                        break;
                    }
                    Err(e) => {
                        error!("Could not get bytes from data: {:?}\n", e);
                        return Err("Could not get bytes from response");
                    }
                }
            }
            Ok(())
        } else {
            error!("Could not open file for writing: {:?}\n", &path);
            Err("Could not open file for writing")
        }
    } else {
        if let Err(e) = client.get(&url).send().await {
            error!("File error for {:?} with error {:#?}\n", &path, &e);
            //error!("Did not work {:?}", &url);
        }
        Err("Could not make request")
    };
    //error!("Worked {:?}", &url);
    bar.inc(1);
    res
}
