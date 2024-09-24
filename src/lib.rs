/*
    Created by Zoltan Kovari, 2024.

    Licensed under the Apache License, Version 2.0
    http://www.apache.org/licenses/LICENSE-2.0
    (see LICENSE.txt)
*/

use std::error::Error;
use std::fmt::Display;
use std::fs::File;
use std::io::prelude::*;
use chrono::prelude::*;


pub struct Config {
    pub key: String,
    pub channel_name: String,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub output: Option<File>
}

#[derive(Debug)]
struct Video {
    date: DateTime<Utc>,
    title: String,
    duration: String,
    id: String
}
impl Display for Video {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{},{},{},{}",
            self.date.to_rfc3339_opts(SecondsFormat::Secs, true),
            self.title,
            self.duration,
            self.id
        )
    }
}

/*
    Working principle:
    1) Get ID based on channel name
        Note: Playlist ID is the same for the default 'Videos' tab (TODO parameterize this)
    2) Get playlist item, i.e. video IDs (response is paginated)
    3) Get content duration for each video
    4) Aggregation
*/

pub fn run(mut config: Config) -> Result<(), Box<dyn Error>> {
    println!("Querying channel info...");

    let addr = format!("https://youtube.googleapis.com/youtube/v3/channels?part=id%2Csnippet%2Cstatistics%2CcontentDetails&forHandle={}&key={}",
        config.channel_name, config.key);

    let json = request(&addr)?;
    write_out(&mut config.output, &json)?;

    let playlist_id = match json.pointer("/pageInfo/totalResults")
        .ok_or("Could not find 'totalResults' field")?
        .as_u64()
        .ok_or("Invalid 'totalResults' format")?
    {
        1 => json.pointer("/items/0/contentDetails/relatedPlaylists/uploads")
                .ok_or("Could not find 'uploads' id field")?
                .as_str()
                .ok_or("Invalid 'uploads' id format")?,
        n => {
            println!("Warning: Number of results is '{}'", n);
            return Ok(());
        }
    };
    println!("Playlist ID exctrated.");

    println!("Querying playlist...");

    let mut video_ids = Vec::<String>::new();
    let mut next_page_token: Option<String> = None;
    let mut total_results;
    loop {
        let addr = format!("https://youtube.googleapis.com/youtube/v3/playlistItems?part=id%2Csnippet&playlistId={}&maxResults=50&pageToken={}&key={}",
            playlist_id, next_page_token.unwrap_or(String::new()), config.key);

        let json = request(&addr)?;
        write_out(&mut config.output, &json)?;

        let array = json.get("items")
            .ok_or("Could not find 'items' array")?
            .as_array()
            .ok_or("Invalid 'items' format")?;

        for e in array {
            video_ids.push(e.pointer("/snippet/resourceId/videoId")
                .ok_or("Could not find 'videoId' field")?
                .as_str()
                .ok_or("Invalid 'videoId' format")?
                .to_string()
            );
        }

        next_page_token = match json.get("nextPageToken") {
            Some(v) => Some(v.as_str()
                .ok_or("Invalid 'nextPageToken' format")?
                .to_string()),
            None => None
        };

        total_results = json.pointer("/pageInfo/totalResults")
            .ok_or("Could not find 'totalResults' field")?
            .as_u64()
            .ok_or("Invalid 'totalResults' format")?;

        if array.is_empty() || next_page_token.is_none() || video_ids.len()>=total_results.try_into()? {
            break;
        };
    }
    println!("Video count: {}", video_ids.len());

    print!("Querying video info");
    std::io::stdout().flush()?;

    let mut videos = Vec::<Video>::new();
    for (i, id) in video_ids.iter().enumerate() {
        let addr = format!("https://youtube.googleapis.com/youtube/v3/videos?part=snippet%2CcontentDetails&id={}&key={}",
            id, config.key);

        let json = request(&addr)?;
        write_out(&mut config.output, &json)?;

        let date = match DateTime::parse_from_rfc3339(
            json.pointer("/items/0/snippet/publishedAt")
                .ok_or("Could not find 'publishedAt' field")?
                .as_str()
                .ok_or("Invalid 'publishedAt' format")?
        ) {
            Ok(d) => DateTime::<Utc>::from(d),
            Err(e) => return Err(format!("Could not parse 'publishedAt' timestamp: {}", e.to_string()))?
        };

        let title = json.pointer("/items/0/snippet/title")
            .ok_or("Could not find 'title' field")?
            .as_str()
            .ok_or("Invalid 'title' format")?
            .to_string();

        let duration = json.pointer("/items/0/contentDetails/duration")
            .ok_or("Could not find 'duration' field")?
            .as_str()
            .ok_or("Invalid 'duration' format")?
            .to_string();

        videos.push(Video {
            date,
            title,
            duration,
            id: id.clone()
        });

        if ((i+1)*10/video_ids.len())>(i*10/video_ids.len()) {
            print!(".");
            std::io::stdout().flush()?;
        }
    }
    println!("");

    if let Some(ref mut out) = config.output {
        out.set_len(0)?;
        out.rewind()?;
        writeln!(out, "#publishedAt,title,duration,videoId")?;
        for v in videos {
            writeln!(out, "{}", v)?
        }
        println!("Success, output written to 'output.txt'.");
    } else {
        println!("Success.");
    }

    Ok(())
}

fn request(address: &str) -> Result<serde_json::Value, Box<dyn Error>> {
    let req: ureq::Request = ureq::get(address).set("Accept", "application/json");

    match req.call() {
        Ok(res) => match res.into_json() {
            Ok(json) => Ok(json),
            Err(e) => return Err(format!("Failed to read JSON: {}", e.to_string()))?
        },
        Err(e) => {
            if let ureq::Error::Status(status, _r) = e {
                return Err(format!("Received HTTP status code: {}", http::StatusCode::from_u16(status).unwrap()))?
            } else {
                return Err(format!("HTTP transfer failure: {}", e.to_string()))?
            }
        }
    }
}

fn write_out(out: &mut Option<File>, item: &impl Display) -> Result<(), Box<dyn Error>> {
    if let Some(ref mut out) = out {
        out.set_len(0)?;
        out.rewind()?;
        write!(out, "{}", item)?
    }
    Ok(())
}
