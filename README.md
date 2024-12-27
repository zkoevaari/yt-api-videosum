yt-api-videosum
===============

Command line interface program written in Rust to extract video duration info 
through the YouTube API, to get the total content duration for a channel in a 
given date period.

## Usage ##
Invoked with the `-h` option, the program displays the following help text:

```
Description:
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
```

## Output ##

As an example, let's sum the videos on the official *@YouTube* channel 
uploaded this year so far:

```
 ./yt-api-videosum -s 2024-01-01T00:00:00Z -e 2024-12-27T17:00:00Z @YouTube
Info: No API key supplied, trying 'config/key.txt' file...
Successfully loaded API key.
Querying channel info...
Playlist ID extracted.
Querying playlist...
Video count: 24
Querying video info..........
Success, output written to 'output.txt'.
Sum total: 27284 seconds, or 7 hours 34 minutes 44 seconds
```

## Motivation ##

If you are like me and watch many educational videos, you might wonder if the
total runtime of a channel could add up to the equivalent of a decent
educational programme...

If interested in this train of thought, see my related blog post where I have 
made comparisons to university course lengths, so I can feel at least a 
little less troubled by ~~wasting~~ spending so much time on YT.

https://www.rockfort.io/blog/2024/241223_yt_api_videosum.html
