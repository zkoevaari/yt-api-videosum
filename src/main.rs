/*
    Created by Zoltan Kovari, 2024.

    Licensed under the Apache License, Version 2.0
    http://www.apache.org/licenses/LICENSE-2.0
    (see LICENSE.txt)
*/

/*
    Motivation:
    If you watch many tutorial videos, you might wonder if total runtime could add up to the equivalent of a decent educational programme...
*/

const DESC: &str =
"Description:
YouTube API tool for calculating the video runtime sum of a channel.

Usage:
yt_api_videosum [-k api_key] [-s [start_date]] [-e [end_date]] [channel_name]

Options:
-k  YT API key supplied in plain text.
      If empty, the program will look for it in the 'config/key.txt' file.
-s
-e  Filter the videos by publish date, giving a start- and/or end date for
      the active interval. Date is expected in RFC3339 format,
      i.e. 'yyyy-mm-ddTHH:MM:SSZ' (note the UTC timezone).
      If the timestamp is empty, it will be asked interactively.
-h  Display this help and exit.

Parameters:
channel_name  Human-readable name of the channel, with or without the
                '@' prefix. If omitted, it will be asked interactively.

Output:
Aggregated total of video duration is displayed interactively.
Also a full list of the videos are saved to 'output.txt' in CSV format, or in
case the process could not complete, it will contain the last intermediate
JSON response to help figuring out what went wrong.

Created by Zoltan Kovari, 2024.
";

/*
    TODO:
    - Filter out shorts, live, private and unlisted
    - Command line option for output file
*/


use std::io::BufRead;
use std::fs::File;
use chrono::prelude::*;


enum OptionalDate {
    Some(String),
    Ask,
    Date(DateTime<Utc>),
    None
}

const HELP: &str = "Run with '-h' option to display help.";


fn main() -> Result<(), Box<dyn std::error::Error>> {

    /* Start loading command line arguments */

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|e| e=="-h" || e=="--help") {
            println!("{}", DESC);
            return Ok(());
    }

    let mut key: Option<String> = None;
    let mut start_date: OptionalDate = OptionalDate::None;
    let mut end_date: OptionalDate = OptionalDate::None;
    let mut channel_name: Option<String> = None;

    let mut i=0;
    while i<args.len() {
        let e = &args[i];

        if e.starts_with('-') {
            match e.as_str() {
                "-k" => {
                    match args.get(i+1) {
                        Some(s) if !s.starts_with('-') => {
                            i += 1;
                            key = Some(String::from(s));
                        },
                        _ => ()
                    };
                },
                "-s" => {
                    start_date = match args.get(i+1) {
                        Some(s) if !s.starts_with('-') => {
                            i += 1;
                            match s.is_empty() {
                                false => OptionalDate::Some(String::from(s)),
                                true => OptionalDate::Ask
                            }
                        },
                        _ => OptionalDate::Ask
                    };
                },
                "-e" => {
                    end_date = match args.get(i+1) {
                        Some(s) if !s.starts_with('-') => {
                            i += 1;
                            match s.is_empty() {
                                false => OptionalDate::Some(String::from(s)),
                                true => OptionalDate::Ask
                            }
                        },
                        _ => OptionalDate::Ask
                    };
                },
                _ => {
                    println!("Warning: Invalid argument(s)!\n{}", HELP);
                    return Ok(());
                }
            }
        } else if i==args.len()-1 {
            channel_name = Some(e.clone());
        } else {
            println!("Warning: Invalid argument(s)!\n{}", HELP);
            return Ok(());
        }

        i += 1;
    }

    /* Parse dates if specified */

    if let OptionalDate::Some(s) = start_date {
        match DateTime::parse_from_rfc3339(&s) {
            Ok(d) => {
                start_date = OptionalDate::Date(DateTime::<Utc>::from(d));
            },
            Err(e) => {
                return Err(format!("Could not parse start timestamp '{}': {}", &s, e.to_string()))?;
            }
        }
    }
    if let OptionalDate::Some(s) = end_date {
        match DateTime::parse_from_rfc3339(&s) {
            Ok(d) => {
                end_date = OptionalDate::Date(DateTime::<Utc>::from(d));
            },
            Err(e) => {
                return Err(format!("Could not parse end timestamp '{}': {}", &s, e.to_string()))?;
            }
        }
    }

    /* Parse or load API key */

    let key = match key {
        Some(k) => k,
        None => {
            println!("Info: No API key supplied, trying 'config/key.txt' file...");
            let file = std::fs::File::open("config/key.txt")?;
            let meta = file.metadata()?;
            if !meta.is_file() {
                return Err("Target is not a regular file".into());
            } else {
                match meta.len() {
                    0 => return Err("File is empty".into()),
                    128.. => return Err(format!("File looks too large to only contain the key [len={}]", meta.len()).into()),
                    _ => {
                        let mut s = String::new();
                        std::io::BufReader::new(file).read_line(&mut s)?;
                        println!("Successfully loaded API key.");
                        match s.trim().split_once(char::is_whitespace) {
                            Some((first, _)) => String::from(first),
                            None => s
                        }
                    }
                }
            }
        }
    };

    /* Ask for channel name if not specified */

    let channel_name = String::from(match channel_name {
        Some(name) => name,
        None => {
            let mut name;
            loop {
                println!("Channel name:");
                name = String::new();
                std::io::stdin().read_line(&mut name)?;
                if name.trim().is_empty() {
                    println!("Warning: Empty name supplied!");
                } else if !name.is_ascii() || name.trim().contains(char::is_whitespace) {
                    println!("Warning: Invalid character supplied!");
                } else {
                    break;
                }
            }
            name
        }
    }.trim().trim_matches('@'));

    /* Ask for dates if needed */

    if let OptionalDate::Ask = start_date {
        loop {
            println!("Filter to dates starting from:");
            let mut s = String::new();
            std::io::stdin().read_line(&mut s)?;
            let s = s.as_str().trim();
            match DateTime::parse_from_rfc3339(&s.trim()) {
                Ok(d) => {
                    start_date = OptionalDate::Date(DateTime::<Utc>::from(d));
                    break;
                },
                Err(e) => {
                    println!("Warning: Could not parse timestamp '{}': {}", &s, e.to_string());
                    println!("Note: RFC3339 format required, i.e. 'yyyy-mm-ddTHH:MM:SSZ'");
                }
            }
        }
    }
    if let OptionalDate::Ask = end_date {
        loop {
            println!("Filter to dates ending at:");
            let mut s = String::new();
            std::io::stdin().read_line(&mut s)?;
            let s = s.as_str().trim();
            match DateTime::parse_from_rfc3339(&s) {
                Ok(d) => {
                    end_date = OptionalDate::Date(DateTime::<Utc>::from(d));
                    break;
                },
                Err(e) => {
                    println!("Warning: Could not parse timestamp '{}': {}", &s, e.to_string());
                    println!("Note: RFC3339 format required, i.e. 'yyyy-mm-ddTHH:MM:SSZ'");
                }
            }
        }
    }

    /* Setup output file writer */

    let output = File::create("output.txt")?;

    /* Config done, lib call */
    //TODO bring out the output stream
    yt_api_videosum::run(
        yt_api_videosum::Config {
            key,
            channel_name,
            start_date: match start_date {
                OptionalDate::Date(d) => Some(d),
                _ => None
            },
            end_date: match end_date {
                OptionalDate::Date(d) => Some(d),
                _ => None
            },
            output: Some(output)
        }
    )
}
