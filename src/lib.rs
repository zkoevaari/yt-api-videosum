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
            println!("Warning: More than one result ({})", n);
            return Ok(());
        }
    };

    //Filtering to public only (ie. excluding shorts, live, private and unlisted) by replacing default "UU" prefix
    let mut playlist_id_pub = String::new();
    playlist_id_pub.push_str("UULF");
    playlist_id_pub.push_str(&playlist_id[2..]);
    println!("Playlist ID extracted.");

    println!("Querying playlist...");

    let mut video_ids = Vec::<String>::new();
    let mut next_page_token: Option<String> = None;
    loop {
        let addr = format!("https://youtube.googleapis.com/youtube/v3/playlistItems?part=id%2Csnippet&playlistId={}&maxResults=50&pageToken={}&key={}",
            playlist_id_pub, next_page_token.unwrap_or(String::new()), config.key);

        let json = request(&addr)?;
        write_out(&mut config.output, &json)?;

        let array = json.get("items")
            .ok_or("Could not find 'items' array")?
            .as_array()
            .ok_or("Invalid 'items' format")?;

        for e in array {
            let date = match DateTime::parse_from_rfc3339(
                e.pointer("/snippet/publishedAt")
                    .ok_or("Could not find 'publishedAt' field")?
                    .as_str()
                    .ok_or("Invalid 'publishedAt' format")?
            ) {
                Ok(d) => DateTime::<Utc>::from(d),
                Err(e) => return Err(format!("Could not parse 'publishedAt' timestamp: {}", e.to_string()))?
            };

            if let Some(start) = config.start_date {
                if date < start {
                    continue;
                }
            }
            if let Some(end) = config.end_date {
                if date > end {
                    continue;
                }
            }

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

        let total_results = json.pointer("/pageInfo/totalResults")
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
        print!(", or {}", dissect_delta(total, TimeBase::Hours));
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

#[derive(Clone, Copy, PartialEq, PartialOrd)]
enum TimeBase {
    _Seconds,
    Minutes,
    Hours,
    Days,
}
fn dissect_delta(mut delta: TimeDelta, base: TimeBase) -> String {
    let plural = |x: i64| -> &str {
        match x {
            1 => "",
            _ => "s"
        }
    };

    let mut out = String::new();

    if delta >= TimeDelta::days(1) && base >= TimeBase::Days {
        let d = delta.num_days();
        out.push_str(format!("{} day{}", d, plural(d)).as_str());
        delta -= TimeDelta::days(d);
    }
    if delta >= TimeDelta::hours(1) && base >= TimeBase::Hours {
        let h = delta.num_hours();
        if h>0 && !out.is_empty() { out.push(' ') }
        out.push_str(format!("{} hour{}", h, plural(h)).as_str());
        delta -= TimeDelta::hours(h);
    }
    if delta >= TimeDelta::minutes(1) && base >= TimeBase::Minutes {
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
        let sec = TimeBase::_Seconds;
        let min = TimeBase::Minutes;
        let hrs = TimeBase::Hours;
        let days = TimeBase::Days;

        let tests = [
            (0, sec, "0 seconds"),
            (1, sec, "1 second"),
            (59, sec, "59 seconds"),
            (60, sec, "60 seconds"),
            (61, sec, "61 seconds"),
            (119, sec, "119 seconds"),
            (120, sec, "120 seconds"),
            (121, sec, "121 seconds"),
            (599, sec, "599 seconds"),
            (600, sec, "600 seconds"),
            (601, sec, "601 seconds"),
            (659, sec, "659 seconds"),
            (660, sec, "660 seconds"),
            (661, sec, "661 seconds"),
            (3599, sec, "3599 seconds"),
            (3600, sec, "3600 seconds"),
            (3601, sec, "3601 seconds"),
            (3659, sec, "3659 seconds"),
            (3660, sec, "3660 seconds"),
            (3661, sec, "3661 seconds"),
            (4199, sec, "4199 seconds"),
            (4200, sec, "4200 seconds"),
            (4201, sec, "4201 seconds"),
            (7199, sec, "7199 seconds"),
            (7200, sec, "7200 seconds"),
            (7201, sec, "7201 seconds"),
            (7259, sec, "7259 seconds"),
            (7260, sec, "7260 seconds"),
            (7261, sec, "7261 seconds"),
            (86399, sec, "86399 seconds"),
            (86400, sec, "86400 seconds"),
            (86401, sec, "86401 seconds"),
            (86459, sec, "86459 seconds"),
            (86460, sec, "86460 seconds"),
            (86461, sec, "86461 seconds"),
            (89999, sec, "89999 seconds"),
            (90000, sec, "90000 seconds"),
            (90001, sec, "90001 seconds"),
            (90059, sec, "90059 seconds"),
            (90060, sec, "90060 seconds"),
            (90061, sec, "90061 seconds"),
            (604799, sec, "604799 seconds"),
            (604800, sec, "604800 seconds"),
            (604801, sec, "604801 seconds"),

            (0, min, "0 seconds"),
            (1, min, "1 second"),
            (59, min, "59 seconds"),
            (60, min, "1 minute"),
            (61, min, "1 minute 1 second"),
            (119, min, "1 minute 59 seconds"),
            (120, min, "2 minutes"),
            (121, min, "2 minutes 1 second"),
            (599, min, "9 minutes 59 seconds"),
            (600, min, "10 minutes"),
            (601, min, "10 minutes 1 second"),
            (659, min, "10 minutes 59 seconds"),
            (660, min, "11 minutes"),
            (661, min, "11 minutes 1 second"),
            (3599, min, "59 minutes 59 seconds"),
            (3600, min, "60 minutes"),
            (3601, min, "60 minutes 1 second"),
            (3659, min, "60 minutes 59 seconds"),
            (3660, min, "61 minutes"),
            (3661, min, "61 minutes 1 second"),
            (4199, min, "69 minutes 59 seconds"),
            (4200, min, "70 minutes"),
            (4201, min, "70 minutes 1 second"),
            (7199, min, "119 minutes 59 seconds"),
            (7200, min, "120 minutes"),
            (7201, min, "120 minutes 1 second"),
            (7259, min, "120 minutes 59 seconds"),
            (7260, min, "121 minutes"),
            (7261, min, "121 minutes 1 second"),
            (86399, min, "1439 minutes 59 seconds"),
            (86400, min, "1440 minutes"),
            (86401, min, "1440 minutes 1 second"),
            (86459, min, "1440 minutes 59 seconds"),
            (86460, min, "1441 minutes"),
            (86461, min, "1441 minutes 1 second"),
            (89999, min, "1499 minutes 59 seconds"),
            (90000, min, "1500 minutes"),
            (90001, min, "1500 minutes 1 second"),
            (90059, min, "1500 minutes 59 seconds"),
            (90060, min, "1501 minutes"),
            (90061, min, "1501 minutes 1 second"),
            (604799, min, "10079 minutes 59 seconds"),
            (604800, min, "10080 minutes"),
            (604801, min, "10080 minutes 1 second"),

            (0, hrs, "0 seconds"),
            (1, hrs, "1 second"),
            (59, hrs, "59 seconds"),
            (60, hrs, "1 minute"),
            (61, hrs, "1 minute 1 second"),
            (119, hrs, "1 minute 59 seconds"),
            (120, hrs, "2 minutes"),
            (121, hrs, "2 minutes 1 second"),
            (599, hrs, "9 minutes 59 seconds"),
            (600, hrs, "10 minutes"),
            (601, hrs, "10 minutes 1 second"),
            (659, hrs, "10 minutes 59 seconds"),
            (660, hrs, "11 minutes"),
            (661, hrs, "11 minutes 1 second"),
            (3599, hrs, "59 minutes 59 seconds"),
            (3600, hrs, "1 hour"),
            (3601, hrs, "1 hour 1 second"),
            (3659, hrs, "1 hour 59 seconds"),
            (3660, hrs, "1 hour 1 minute"),
            (3661, hrs, "1 hour 1 minute 1 second"),
            (4199, hrs, "1 hour 9 minutes 59 seconds"),
            (4200, hrs, "1 hour 10 minutes"),
            (4201, hrs, "1 hour 10 minutes 1 second"),
            (7199, hrs, "1 hour 59 minutes 59 seconds"),
            (7200, hrs, "2 hours"),
            (7201, hrs, "2 hours 1 second"),
            (7259, hrs, "2 hours 59 seconds"),
            (7260, hrs, "2 hours 1 minute"),
            (7261, hrs, "2 hours 1 minute 1 second"),
            (86399, hrs, "23 hours 59 minutes 59 seconds"),
            (86400, hrs, "24 hours"),
            (86401, hrs, "24 hours 1 second"),
            (86459, hrs, "24 hours 59 seconds"),
            (86460, hrs, "24 hours 1 minute"),
            (86461, hrs, "24 hours 1 minute 1 second"),
            (89999, hrs, "24 hours 59 minutes 59 seconds"),
            (90000, hrs, "25 hours"),
            (90001, hrs, "25 hours 1 second"),
            (90059, hrs, "25 hours 59 seconds"),
            (90060, hrs, "25 hours 1 minute"),
            (90061, hrs, "25 hours 1 minute 1 second"),
            (604799, hrs, "167 hours 59 minutes 59 seconds"),
            (604800, hrs, "168 hours"),
            (604801, hrs, "168 hours 1 second"),

            (0, days, "0 seconds"),
            (1, days, "1 second"),
            (59, days, "59 seconds"),
            (60, days, "1 minute"),
            (61, days, "1 minute 1 second"),
            (119, days, "1 minute 59 seconds"),
            (120, days, "2 minutes"),
            (121, days, "2 minutes 1 second"),
            (599, days, "9 minutes 59 seconds"),
            (600, days, "10 minutes"),
            (601, days, "10 minutes 1 second"),
            (659, days, "10 minutes 59 seconds"),
            (660, days, "11 minutes"),
            (661, days, "11 minutes 1 second"),
            (3599, days, "59 minutes 59 seconds"),
            (3600, days, "1 hour"),
            (3601, days, "1 hour 1 second"),
            (3659, days, "1 hour 59 seconds"),
            (3660, days, "1 hour 1 minute"),
            (3661, days, "1 hour 1 minute 1 second"),
            (4199, days, "1 hour 9 minutes 59 seconds"),
            (4200, days, "1 hour 10 minutes"),
            (4201, days, "1 hour 10 minutes 1 second"),
            (7199, days, "1 hour 59 minutes 59 seconds"),
            (7200, days, "2 hours"),
            (7201, days, "2 hours 1 second"),
            (7259, days, "2 hours 59 seconds"),
            (7260, days, "2 hours 1 minute"),
            (7261, days, "2 hours 1 minute 1 second"),
            (86399, days, "23 hours 59 minutes 59 seconds"),
            (86400, days, "1 day"),
            (86401, days, "1 day 1 second"),
            (86459, days, "1 day 59 seconds"),
            (86460, days, "1 day 1 minute"),
            (86461, days, "1 day 1 minute 1 second"),
            (89999, days, "1 day 59 minutes 59 seconds"),
            (90000, days, "1 day 1 hour"),
            (90001, days, "1 day 1 hour 1 second"),
            (90059, days, "1 day 1 hour 59 seconds"),
            (90060, days, "1 day 1 hour 1 minute"),
            (90061, days, "1 day 1 hour 1 minute 1 second"),
            (604799, days, "6 days 23 hours 59 minutes 59 seconds"),
            (604800, days, "7 days"),
            (604801, days, "7 days 1 second"),
        ];

        for (t, b, s) in tests {
            assert_eq!(dissect_delta(TimeDelta::seconds(t), b), s);
        }
    }
}
