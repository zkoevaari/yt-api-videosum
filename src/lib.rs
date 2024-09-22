/*
    Created by Zoltan Kovari, 2024.

    Licensed under the Apache License, Version 2.0
    http://www.apache.org/licenses/LICENSE-2.0
    (see LICENSE.txt)
*/

use std::error::Error;
use std::io::Write;
use std::fs::File;
use chrono::prelude::*;


#[derive(Debug)]
pub struct Config {
    pub key: String,
    pub channel_name: String,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub output: Option<File>
}

/*
    Working principle:
    1) Get ID based on channel name
        Note: Playlist ID is the same for the default 'Videos' tab (TODO parameterize this)
    2) Get playlist item, i.e. video IDs (response is paginated)
    3) Get content duration for each video
    4) Aggregation
*/

pub fn run(config: Config) -> Result<(), Box<dyn Error>> {
    println!("Querying channel info...");

    let addr_channel_info = format!("https://youtube.googleapis.com/youtube/v3/channels?part=id%2Csnippet%2Cstatistics%2CcontentDetails&forHandle={}&key={}",
        config.channel_name, config.key);
    let req: ureq::Request = ureq::get(&addr_channel_info).set("Accept", "application/json");

    let json: serde_json::Value = match req.call() {
        Ok(res) => match res.into_json() {
            Ok(json) => json,
            Err(e) => return Err(format!("Failed to read JSON: {}", e.to_string()))?
        },
        Err(e) => {
            if let ureq::Error::Status(status, _r) = e {
                return Err(format!("Received HTTP status code: {}", http::StatusCode::from_u16(status).unwrap()))?
            } else {
                return Err(format!("HTTP transfer failure: {}", e.to_string()))?
            }
        }
    };

    if let Some(mut out) = config.output {write!(out, "{}", json)?}

    let channel_id = match json.pointer("/pageInfo/totalResults")
        .ok_or("Could not find 'totalResults' field")?
        .as_u64()
        .ok_or("Invalid 'totalResults' format")?
    {
        1 => json.pointer("/items/0/id")
                .ok_or("Could not find 'id' field")?
                .as_str()
                .ok_or("Invalid 'id' format")?,
        n => {
            println!("Warning: Number of results is '{}'", n);
            return Ok(());
        }
    };
    println!("Channel ID exctrated.");

//~     let playlist_info = "https://youtube.googleapis.com/youtube/v3/playlistItems?part=id%2Csnippet&playlistId=UUfsznjef2zGJnrCRQBXqo6Q&maxResults=50&key=AIzaSyBclsgOYqmmfTOGUoMm42HqxUlK8iRouIg";
//~     let video_info = "https://youtube.googleapis.com/youtube/v3/videos?part=snippet%2CcontentDetails&id=gNRnrn5DE58&key=AIzaSyBclsgOYqmmfTOGUoMm42HqxUlK8iRouIg";

    Ok(())
}
