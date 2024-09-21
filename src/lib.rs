/*
    Created by Zoltan Kovari, 2024.

    Licensed under the Apache License, Version 2.0
    http://www.apache.org/licenses/LICENSE-2.0
    (see LICENSE.txt)
*/

use chrono::prelude::*;


#[derive(Debug)]
pub struct Config {
    pub key: String,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub channel_name: String
}

/*
    Working principle:
    1) Get ID based on channel name
        Note: Playlist ID is the same for the default 'Videos' tab (TODO parameterize this)
    2) Get playlist item, i.e. video IDs (response is paginated)
    3) Get content duration for each video
    4) Aggregation
*/

pub fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    println!("Function 'run' ran with config: {:?}", config);

    Ok(())
}
