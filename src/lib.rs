/*
    Created by Zoltan Kovari, 2024.

    Licensed under the Apache License, Version 2.0
    http://www.apache.org/licenses/LICENSE-2.0
    (see LICENSE.txt)
*/

use std::error::Error;
use std::fmt::Display;
use std::fs::File;
use std::io::{Seek, Write};
use chrono::{DateTime,TimeDelta,Utc,SecondsFormat};


mod period;

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
    id: String,
    duration: String,
    delta: TimeDelta,
}
impl Video {
    fn new(
        date: DateTime<Utc>,
        title: String,
        id: String,
        duration: String,
    ) -> Result<Self, String> {
        let delta = crate::period::parse_delta(duration.as_str())
            .ok_or("Could not parse 'duration' field")?;
        Ok(Self {
            date,
            title,
            id,
            duration,
            delta,
        })
    }
}
impl Display for Video {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{},{},{},{},{}",
            self.date.to_rfc3339_opts(SecondsFormat::Secs, true),
            self.title,
            self.id,
            self.duration,
            self.delta.num_seconds(),
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

        videos.push(
            Video::new(
                date,
                title,
                id.clone(),
                duration,
            )?
        );

        if ((i+1)*10/video_ids.len())>(i*10/video_ids.len()) {
            print!(".");
            std::io::stdout().flush()?;
        }
    }
    println!("");

    if let Some(ref mut out) = config.output {
        out.set_len(0)?;
        out.rewind()?;
        writeln!(out, "#publishedAt,title,videoId,duration,duration_seconds")?;
        for v in &videos {
            writeln!(out, "{}", v)?
        }
        println!("Success, output written to 'output.txt'.");
    } else {
        println!("Success.");
    }

    let total = videos.iter().fold(TimeDelta::zero(), |acc, v| acc + v.delta);
    print!("Sum total: {} seconds", total.num_seconds());
    if total >= TimeDelta::minutes(1) {
        print!(", or {}", dissect_delta(total));
    }
    println!("");

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

fn dissect_delta(mut delta: TimeDelta) -> String {
    let plural = |x: i64| -> &str {
        match x {
            1 => "",
            _ => "s"
        }
    };

    let mut out = String::new();

    if delta >= TimeDelta::days(1) {
        let d = delta.num_days();
        out.push_str(format!("{} day{}", d, plural(d)).as_str());
        delta -= TimeDelta::days(d);
    }
    if delta >= TimeDelta::hours(1) {
        let h = delta.num_hours();
        if h>0 && !out.is_empty() { out.push(' ') }
        out.push_str(format!("{} hour{}", h, plural(h)).as_str());
        delta -= TimeDelta::hours(h);
    }
    if delta >= TimeDelta::minutes(1) {
        let m = delta.num_minutes();
        if m>0 && !out.is_empty() { out.push(' ') }
        out.push_str(format!("{} minute{}", m, plural(m)).as_str());
        delta -= TimeDelta::minutes(m);
    }

    let s = delta.num_seconds();
    if s>0 || out.is_empty() {
        if !out.is_empty() { out.push(' ') }
        out.push_str(format!("{} second{}", s, plural(s)).as_str());
    }
    delta -= TimeDelta::seconds(s);
    debug_assert!(delta < TimeDelta::seconds(1));

    out
}

#[cfg(test)]
mod lib_test {
    use super::*;


    #[test]
    fn dissect_test() {
        let tests = [
            (0, "0 seconds"),
            (1, "1 second"),
            (59, "59 seconds"),
            (60, "1 minute"),
            (61, "1 minute 1 second"),
            (119, "1 minute 59 seconds"),
            (120, "2 minutes"),
            (121, "2 minutes 1 second"),
            (599, "9 minutes 59 seconds"),
            (600, "10 minutes"),
            (601, "10 minutes 1 second"),
            (659, "10 minutes 59 seconds"),
            (660, "11 minutes"),
            (661, "11 minutes 1 second"),
            (3599, "59 minutes 59 seconds"),
            (3600, "1 hour"),
            (3601, "1 hour 1 second"),
            (3659, "1 hour 59 seconds"),
            (3660, "1 hour 1 minute"),
            (3661, "1 hour 1 minute 1 second"),
            (4199, "1 hour 9 minutes 59 seconds"),
            (4200, "1 hour 10 minutes"),
            (4201, "1 hour 10 minutes 1 second"),
            (7199, "1 hour 59 minutes 59 seconds"),
            (7200, "2 hours"),
            (7201, "2 hours 1 second"),
            (7259, "2 hours 59 seconds"),
            (7260, "2 hours 1 minute"),
            (7261, "2 hours 1 minute 1 second"),
            (86399, "23 hours 59 minutes 59 seconds"),
            (86400, "1 day"),
            (86401, "1 day 1 second"),
            (86459, "1 day 59 seconds"),
            (86460, "1 day 1 minute"),
            (86461, "1 day 1 minute 1 second"),
            (89999, "1 day 59 minutes 59 seconds"),
            (90000, "1 day 1 hour"),
            (90001, "1 day 1 hour 1 second"),
            (90059, "1 day 1 hour 59 seconds"),
            (90060, "1 day 1 hour 1 minute"),
            (90061, "1 day 1 hour 1 minute 1 second"),
            (604799, "6 days 23 hours 59 minutes 59 seconds"),
            (604800, "7 days"),
            (604801, "7 days 1 second"),
        ];

        for (t, s) in tests {
//~             println!("{} = {}", t, dissect_delta(TimeDelta::seconds(t)));
            assert_eq!(dissect_delta(TimeDelta::seconds(t)), s);
        }
    }
}
