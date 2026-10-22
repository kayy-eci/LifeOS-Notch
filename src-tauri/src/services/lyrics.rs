//! LRCLIB lyrics.
//!
//! LRCLIB (<https://lrclib.net>) is a free, no-auth lyrics database. The
//! service:
//!
//! * asks for a *synced* lyric first, falling back to plain text;
//! * caches every successful response in SQLite, so a track is fetched once;
//! * caches "not found" too, so a track with no lyrics does not produce a
//!   request every time the panel reopens;
//! * never pretends untimed lyrics are synchronised — `synced: false` is
//!   propagated all the way to the UI.
//!
//! Requests are made with a descriptive `User-Agent`, as the LRCLIB API asks.

use std::time::Duration;

use rusqlite::params;
use serde::Deserialize;

use crate::db::Database;
use crate::error::{AppError, AppResult};
use crate::types::{now_ms, Lyrics, LyricLine};

const API_BASE: &str = "https://lrclib.net/api";
const USER_AGENT: &str = concat!(
    "LifeOSNotch/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/lifeos-notch/lifeos-notch)"
);
const REQUEST_TIMEOUT_SECS: u64 = 8;
/// A cached result is trusted for a week. Lyrics rarely change, and a stale
/// entry is far better than a request on every track change.
const CACHE_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;

#[derive(Debug, Deserialize)]
struct LrcLibTrack {
    #[serde(rename = "trackName", default)]
    track_name: String,
    #[serde(rename = "artistName", default)]
    artist_name: String,
    #[serde(rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
    #[serde(rename = "plainLyrics")]
    plain_lyrics: Option<String>,
    #[serde(rename = "instrumental", default)]
    instrumental: bool,
}

#[derive(Clone, Default)]
pub struct LyricsService {
    client: Option<reqwest::blocking::Client>,
}

impl LyricsService {
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .ok();
        LyricsService { client }
    }

    /// Fetches lyrics, using and filling the local cache.
    pub fn get(
        &self,
        db: &Database,
        track_id: &str,
        title: &str,
        artist: &str,
    ) -> AppResult<Option<Lyrics>> {
        if let Some(cached) = read_cache(db, track_id)? {
            return Ok(cached);
        }

        let fetched = self.fetch(title, artist)?;
        write_cache(db, track_id, fetched.as_ref())?;
        Ok(fetched)
    }

    fn fetch(&self, title: &str, artist: &str) -> AppResult<Option<Lyrics>> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::msg("The lyrics client could not be created."))?;

        let response = client
            .get(format!("{API_BASE}/get"))
            .query(&[("track_name", title), ("artist_name", artist)])
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    AppError::msg("The lyrics request timed out. Check your connection and try again.")
                } else {
                    AppError::msg("Could not reach LRCLIB. Check your connection and try again.")
                }
            })?;

        // 404 is LRCLIB's way of saying "no lyrics for this track", which is a
        // normal outcome rather than a failure.
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(AppError::msg(format!(
                "LRCLIB returned an error ({}). Try again later.",
                response.status().as_u16()
            )));
        }

        let track: LrcLibTrack = response
            .json()
            .map_err(|_| AppError::msg("LRCLIB returned a response this app could not read."))?;

        Ok(build_lyrics(&track))
    }
}

/// Turns an API response into the app's lyrics model.
fn build_lyrics(track: &LrcLibTrack) -> Option<Lyrics> {
    if track.instrumental {
        // An explicit instrumental result is a definite answer, not a miss:
        // the caller shows "This track is instrumental" rather than "no lyrics".
        return Some(Lyrics {
            track_id: String::new(),
            title: track.track_name.clone(),
            artist: track.artist_name.clone(),
            synced: true,
            lines: Vec::new(),
            source_url: None,
        });
    }

    if let Some(synced) = track.synced_lyrics.as_deref() {
        let lines = parse_lrc(synced);
        if !lines.is_empty() {
            return Some(Lyrics {
                track_id: String::new(),
                title: track.track_name.clone(),
                artist: track.artist_name.clone(),
                synced: true,
                lines,
                source_url: None,
            });
        }
    }

    if let Some(plain) = track.plain_lyrics.as_deref() {
        let lines: Vec<LyricLine> = plain
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|text| LyricLine {
                time_ms: -1,
                text: text.to_string(),
            })
            .collect();
        if !lines.is_empty() {
            return Some(Lyrics {
                track_id: String::new(),
                title: track.track_name.clone(),
                artist: track.artist_name.clone(),
                synced: false,
                lines,
                source_url: None,
            });
        }
    }

    None
}

/// Parses an LRC document into timestamped lines.
///
/// LRC allows several timestamps on one line (`[00:10.00][01:20.00]chorus`),
/// because a repeated chorus shares one line of text. Each timestamp becomes
/// its own entry so the active line can be found by binary search.
///
/// Untimed metadata tags (`[ar:Artist]`) are skipped. Lines without any
/// timestamp are dropped: they cannot be positioned on the timeline, and
/// showing them at position zero would be a lie about synchronisation.
pub fn parse_lrc(input: &str) -> Vec<LyricLine> {
    let mut lines = Vec::new();

    for raw in input.lines() {
        let mut rest = raw.trim_start();
        let mut timestamps: Vec<i64> = Vec::new();

        // Consume every leading `[mm:ss.xx]` / `[mm:ss]` / `[mm:ss.xxx]`.
        loop {
            let Some(stripped) = rest.strip_prefix('[') else { break };
            let Some(close) = stripped.find(']') else { break };
            let tag = &stripped[..close];
            match parse_timestamp(tag) {
                Some(ms) => {
                    timestamps.push(ms);
                    rest = &stripped[close + 1..];
                }
                // A non-timestamp tag (id, ar, al, by, offset…) ends the run.
                None => break,
            }
        }

        let text = rest.trim();
        for time_ms in timestamps {
            lines.push(LyricLine {
                time_ms,
                text: text.to_string(),
            });
        }
    }

    // Later timestamps can appear out of order in hand-edited files.
    lines.sort_by_key(|line| line.time_ms);
    lines
}

/// Parses `mm:ss`, `mm:ss.xx` or `mm:ss.xxx` into milliseconds.
fn parse_timestamp(tag: &str) -> Option<i64> {
    let (minutes, rest) = tag.split_once(':')?;
    let minutes: i64 = minutes.trim().parse().ok()?;

    let (seconds, fraction) = match rest.split_once('.') {
        Some((s, f)) => (s, Some(f)),
        None => (rest, None),
    };
    let seconds: i64 = seconds.trim().parse().ok()?;

    let millis = match fraction {
        None => 0,
        Some(f) => {
            let digits: String = f.chars().take(3).collect();
            if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            // `.5` means 500 ms, not 5 ms.
            let value: i64 = digits.parse().ok()?;
            match digits.len() {
                1 => value * 100,
                2 => value * 10,
                _ => value,
            }
        }
    };

    if seconds >= 60 {
        return None;
    }
    Some(minutes * 60_000 + seconds * 1_000 + millis)
}

/* -------------------------------------------------------------------------- */
/* Cache                                                                       */
/* -------------------------------------------------------------------------- */

/// Reads a cached entry. `Ok(None)` means "not cached"; a cached miss is
/// stored as an empty line list with `synced = false` and `source_url` NULL.
fn read_cache(db: &Database, track_id: &str) -> AppResult<Option<Option<Lyrics>>> {
    let conn = db.lock()?;
    let row = conn
        .query_row(
            "SELECT title, artist, synced, lines, source_url, fetched_at
             FROM lyrics_cache WHERE track_id = ?1",
            params![track_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? != 0,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .ok();

    let Some((title, artist, synced, lines_json, source_url, fetched_at)) = row else {
        return Ok(None);
    };

    if now_ms() - fetched_at > CACHE_TTL_MS {
        return Ok(None);
    }

    let lines: Vec<LyricLine> = serde_json::from_str(&lines_json).unwrap_or_default();
    // An empty cached result is a cached "no lyrics".
    if lines.is_empty() && source_url.is_none() && !synced {
        return Ok(Some(None));
    }

    Ok(Some(Some(Lyrics {
        track_id: track_id.to_string(),
        title,
        artist,
        synced,
        lines,
        source_url,
    })))
}

fn write_cache(db: &Database, track_id: &str, lyrics: Option<&Lyrics>) -> AppResult<()> {
    let (title, artist, synced, lines, source) = match lyrics {
        Some(l) => (
            l.title.as_str(),
            l.artist.as_str(),
            l.synced,
            serde_json::to_string(&l.lines)?,
            l.source_url.clone(),
        ),
        None => ("", "", false, "[]".to_string(), None),
    };

    let conn = db.lock()?;
    conn.execute(
        "INSERT INTO lyrics_cache (track_id, title, artist, synced, lines, source_url, fetched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(track_id) DO UPDATE SET
            title = excluded.title,
            artist = excluded.artist,
            synced = excluded.synced,
            lines = excluded.lines,
            source_url = excluded.source_url,
            fetched_at = excluded.fetched_at",
        params![track_id, title, artist, synced as i64, lines, source, now_ms()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_lrc_timestamps() {
        let lrc = "[00:00.00]First line\n[00:12.34]Second line\n[01:02.50]Third line";
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].time_ms, 0);
        assert_eq!(lines[1].time_ms, 12_340);
        assert_eq!(lines[2].time_ms, 62_500);
        assert_eq!(lines[2].text, "Third line");
    }

    #[test]
    fn parses_short_fractions_correctly() {
        // `.5` is half a second, not five milliseconds.
        assert_eq!(parse_timestamp("00:01.5"), Some(1_500));
        assert_eq!(parse_timestamp("00:01.05"), Some(1_050));
        assert_eq!(parse_timestamp("00:01.005"), Some(1_005));
        assert_eq!(parse_timestamp("00:01"), Some(1_000));
    }

    #[test]
    fn skips_metadata_tags() {
        let lrc = "[ar:Some Artist]\n[ti:Some Title]\n[by:someone]\n[00:05.00]Real line";
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Real line");
    }

    #[test]
    fn handles_repeated_chorus_timestamps() {
        // One line of text with two timestamps becomes two entries.
        let lrc = "[00:10.00][00:50.00]Chorus";
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].time_ms, 10_000);
        assert_eq!(lines[1].time_ms, 50_000);
        assert_eq!(lines[0].text, "Chorus");
        assert_eq!(lines[1].text, "Chorus");
    }

    #[test]
    fn drops_untimed_lines() {
        let lrc = "[00:01.00]Timed\nUntimed line\n[00:02.00]Also timed";
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|l| l.time_ms >= 0));
    }

    #[test]
    fn sorts_out_of_order_timestamps() {
        let lrc = "[00:30.00]Later\n[00:10.00]Earlier";
        let lines = parse_lrc(lrc);
        assert_eq!(lines[0].text, "Earlier");
        assert_eq!(lines[1].text, "Later");
    }

    #[test]
    fn rejects_invalid_timestamps() {
        assert_eq!(parse_timestamp("no-colon"), None);
        assert_eq!(parse_timestamp("00:75"), None); // seconds out of range
    }

    #[test]
    fn empty_input_produces_no_lines() {
        assert!(parse_lrc("").is_empty());
        assert!(parse_lrc("\n\n\n").is_empty());
    }

    #[test]
    fn synced_lyrics_are_preferred_over_plain() {
        let track = LrcLibTrack {
            track_name: "T".into(),
            artist_name: "A".into(),
            synced_lyrics: Some("[00:01.00]Synced".into()),
            plain_lyrics: Some("Plain".into()),
            instrumental: false,
        };
        let lyrics = build_lyrics(&track).unwrap();
        assert!(lyrics.synced);
        assert_eq!(lyrics.lines[0].text, "Synced");
    }

    #[test]
    fn plain_lyrics_are_marked_as_unsynced() {
        let track = LrcLibTrack {
            track_name: "T".into(),
            artist_name: "A".into(),
            synced_lyrics: None,
            plain_lyrics: Some("Line one\nLine two".into()),
            instrumental: false,
        };
        let lyrics = build_lyrics(&track).unwrap();
        assert!(!lyrics.synced);
        assert_eq!(lyrics.lines.len(), 2);
    }

    #[test]
    fn instrumental_tracks_are_distinguished_from_missing_lyrics() {
        let track = LrcLibTrack {
            track_name: "T".into(),
            artist_name: "A".into(),
            synced_lyrics: None,
            plain_lyrics: None,
            instrumental: true,
        };
        let lyrics = build_lyrics(&track).unwrap();
        assert!(lyrics.lines.is_empty());
    }

    #[test]
    fn no_lyrics_at_all_is_none() {
        let track = LrcLibTrack {
            track_name: "T".into(),
            artist_name: "A".into(),
            synced_lyrics: None,
            plain_lyrics: None,
            instrumental: false,
        };
        assert!(build_lyrics(&track).is_none());
    }
}
