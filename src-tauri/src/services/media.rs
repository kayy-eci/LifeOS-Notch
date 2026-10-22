//! Windows media session integration, via the Global System Media Transport
//! Controls (GSMTC) API.
//!
//! # Design
//!
//! The service keeps one cached snapshot of the current session and refreshes
//! it on a timer. That is deliberate: GSMTC exposes change events, but the
//! event stream is unreliable for some players (browser players in particular
//! fire `PlaybackInfoChanged` far more often than anything actually changes),
//! and a snapshot comparison is the only way to avoid pushing a React update
//! every time a player reports the same metadata again.
//!
//! Polling is only active while a session exists. When no player is open the
//! service stops doing work entirely, so an idle machine with no music costs
//! nothing.
//!
//! # What is deliberately *not* invented
//!
//! * A player that exposes no title gets `None`, not "Unknown track".
//! * A player that reports no duration gets `None`, and the UI hides the
//!   progress bar instead of drawing a fake one.
//! * Album art is only fetched when the track changes, and only decoded once.

use std::sync::{Arc, Mutex};

use crate::error::AppResult;
use crate::types::{MediaCommand, MediaSession, PlaybackStatus};

/// The service's shared snapshot. Cheap to clone, so the ticker thread and the
/// command handlers read the same value.
#[derive(Debug, Clone)]
pub struct MediaService {
    inner: Arc<Mutex<MediaInner>>,
}

#[derive(Debug, Default)]
struct MediaInner {
    /// The last snapshot pushed to the UI. Used to suppress no-op updates.
    last: Option<MediaSession>,
    /// The track the cached artwork belongs to, so artwork is not re-decoded
    /// on every poll.
    artwork_key: Option<String>,
    artwork: Option<String>,
}

impl Default for MediaService {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaService {
    pub fn new() -> Self {
        MediaService {
            inner: Arc::new(Mutex::new(MediaInner::default())),
        }
    }

    /// The current session, read fresh from Windows.
    pub fn current_session(&self) -> AppResult<Option<MediaSession>> {
        imp::current_session(self)
    }

    /// Sends a transport command to the current session.
    pub fn command(&self, command: MediaCommand) -> AppResult<Option<MediaSession>> {
        imp::send_command(command)?;
        // The player updates asynchronously; report the state as it is now and
        // let the next poll pick up the change.
        self.current_session()
    }

    /// Seeks to an absolute position within the track.
    pub fn seek(&self, position_seconds: f64) -> AppResult<Option<MediaSession>> {
        imp::seek(position_seconds)?;
        self.current_session()
    }

    /// Returns the new snapshot only when it differs from the last one.
    ///
    /// This is what keeps the frontend from re-rendering on every poll: a
    /// player that reports the same title and the same rounded position
    /// produces no event at all.
    pub fn poll(&self) -> AppResult<Option<Option<MediaSession>>> {
        let current = self.current_session()?;
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| crate::error::AppError::msg("The media state is busy."))?;

        if sessions_equal(inner.last.as_ref(), current.as_ref()) {
            return Ok(None);
        }

        inner.last = current.clone();
        Ok(Some(current))
    }

    /// The cached artwork for a track key, if it has already been fetched.
    pub fn cached_artwork(&self, key: &str) -> Option<String> {
        let inner = self.inner.lock().ok()?;
        if inner.artwork_key.as_deref() == Some(key) {
            inner.artwork.clone()
        } else {
            None
        }
    }

    pub fn store_artwork(&self, key: String, artwork: Option<String>) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.artwork_key = Some(key);
            inner.artwork = artwork;
        }
    }
}

/// Compares two snapshots for the purpose of suppressing updates.
///
/// The playback position is compared after rounding to the nearest second:
/// a player that reports `12.001` and then `12.4` has not changed in any way
/// the user can perceive, and pushing that to React 20 times a second would
/// make the whole notch stutter.
fn sessions_equal(a: Option<&MediaSession>, b: Option<&MediaSession>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.app_id == b.app_id
                && a.title == b.title
                && a.artist == b.artist
                && a.album == b.album
                && a.status == b.status
                && a.duration_seconds == b.duration_seconds
                && a.position_seconds.round() == b.position_seconds.round()
        }
        _ => false,
    }
}

/* -------------------------------------------------------------------------- */
/* Pure helpers                                                                */
/* -------------------------------------------------------------------------- */

/// Identifies the image type from its magic bytes.
///
/// Kept outside the platform module because it has no Windows dependency and
/// is unit tested directly.
fn sniff_image_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Standard base64. Kept local to avoid a dependency for one call site.
fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(TABLE[(triple >> 18) as usize & 0x3F] as char);
        out.push(TABLE[(triple >> 12) as usize & 0x3F] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(triple >> 6) as usize & 0x3F] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[triple as usize & 0x3F] as char
        } else {
            '='
        });
    }
    out
}

/* -------------------------------------------------------------------------- */
/* Windows implementation                                                      */
/* -------------------------------------------------------------------------- */

#[cfg(windows)]
mod imp {
    use windows::core::HSTRING;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager as SessionManager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackState,
    };
    use windows::Storage::Streams::{DataReader, IRandomAccessStreamReference};
    use windows::Foundation::TimeSpan;

    use super::MediaService;
    use crate::error::{AppError, AppResult};
    use crate::types::{MediaCommand, MediaSession, PlaybackStatus};

    /// Reads the manager, which is the entry point for every other call.
    ///
    /// `windows-future` exposes `join()` rather than the old blocking `.get()`:
    /// it waits on the async operation and returns its result.
    fn manager() -> AppResult<SessionManager> {
        SessionManager::RequestAsync()
            .map_err(|e| AppError::media(format!("Windows media controls are unavailable: {e}")))?
            .join()
            .map_err(|e| AppError::media(format!("Windows media controls are unavailable: {e}")))
    }

    fn to_playback_status(value: PlaybackState) -> PlaybackStatus {
        if value == PlaybackState::Playing {
            PlaybackStatus::Playing
        } else if value == PlaybackState::Paused {
            PlaybackStatus::Paused
        } else if value == PlaybackState::Changing {
            PlaybackStatus::Changing
        } else {
            PlaybackStatus::Stopped
        }
    }

    fn hstring_to_option(value: windows::core::Result<HSTRING>) -> Option<String> {
        value.ok().map(|s| s.to_string()).filter(|s| !s.trim().is_empty())
    }

    fn timespan_seconds(value: TimeSpan) -> f64 {
        // `TimeSpan` counts 100-nanosecond intervals.
        value.Duration as f64 / 10_000_000.0
    }

    pub fn current_session(service: &MediaService) -> AppResult<Option<MediaSession>> {
        let manager = match manager() {
            Ok(m) => m,
            // A machine with no media subsystem at all is not an error worth
            // surfacing: the music panel simply has nothing to show.
            Err(_) => return Ok(None),
        };

        let session = match manager.GetCurrentSession() {
            Ok(s) => s,
            Err(_) => return Ok(None),
        };

        let app_id = session
            .SourceAppUserModelId()
            .map(|s| s.to_string())
            .unwrap_or_default();

        let playback = session.GetPlaybackInfo().ok();
        let status = playback
            .as_ref()
            .and_then(|p| p.PlaybackStatus().ok())
            .map(to_playback_status)
            .unwrap_or(PlaybackStatus::Stopped);

        let controls = playback.as_ref().and_then(|p| p.Controls().ok());
        let (can_play, can_pause, can_skip_next, can_skip_previous, can_seek) = match &controls {
            Some(c) => (
                c.IsPlayEnabled().unwrap_or(false),
                c.IsPauseEnabled().unwrap_or(false),
                c.IsNextEnabled().unwrap_or(false),
                c.IsPreviousEnabled().unwrap_or(false),
                c.IsPlaybackPositionEnabled().unwrap_or(false),
            ),
            None => (false, false, false, false, false),
        };

        let properties = session.TryGetMediaPropertiesAsync().ok().and_then(|op| op.join().ok());
        let (title, artist, album) = match &properties {
            Some(p) => (
                hstring_to_option(p.Title()),
                hstring_to_option(p.Artist()),
                hstring_to_option(p.AlbumTitle()),
            ),
            None => (None, None, None),
        };

        let (position_seconds, duration_seconds) = session
            .GetTimelineProperties()
            .map(|timeline| {
                let position = timeline.Position().map(timespan_seconds).unwrap_or(0.0);
                let end = timeline.EndTime().map(timespan_seconds).unwrap_or(0.0);
                let start = timeline.StartTime().map(timespan_seconds).unwrap_or(0.0);
                // Some players report an end time of zero when they do not
                // know the duration. That is `None`, not a zero-length track.
                let duration = if end > start { Some(end - start) } else { None };
                (position.max(0.0), duration)
            })
            .unwrap_or((0.0, None));

        // Artwork is only decoded when the track changes. Re-decoding it on
        // every poll would cost real CPU for no visible benefit.
        let artwork_key = format!(
            "{}|{}|{}",
            app_id,
            title.clone().unwrap_or_default(),
            artist.clone().unwrap_or_default()
        );
        let artwork = match service.cached_artwork(&artwork_key) {
            Some(cached) => cached,
            None => {
                let fetched = properties
                    .as_ref()
                    .and_then(|p| p.Thumbnail().ok())
                    .and_then(read_thumbnail);
                service.store_artwork(artwork_key, fetched.clone());
                fetched.unwrap_or_default()
            }
        };

        Ok(Some(MediaSession {
            app_id,
            title,
            artist,
            album,
            // An empty string means "this player exposes no artwork"; the UI
            // renders its placeholder instead.
            artwork: if artwork.is_empty() { None } else { Some(artwork) },
            status,
            position_seconds,
            duration_seconds,
            can_play,
            can_pause,
            can_skip_next,
            can_skip_previous,
            can_seek,
        }))
    }

    /// Reads the thumbnail stream into a `data:` URL.
    fn read_thumbnail(reference: IRandomAccessStreamReference) -> Option<String> {
        let stream = reference.OpenReadAsync().ok()?.join().ok()?;
        let size = stream.Size().ok()? as u32;
        if size == 0 || size > 8 * 1024 * 1024 {
            // An implausibly large thumbnail is not worth the memory; the UI
            // falls back to the placeholder.
            return None;
        }

        let reader = DataReader::CreateDataReader(&stream).ok()?;
        reader.LoadAsync(size).ok()?.join().ok()?;

        let mut bytes = vec![0u8; size as usize];
        reader.ReadBytes(&mut bytes).ok()?;

        let mime = match super::sniff_image_type(&bytes) {
            Some(m) => m,
            None => return None,
        };
        Some(format!("data:{mime};base64,{}", super::base64_encode(&bytes)))
    }

    pub fn send_command(command: MediaCommand) -> AppResult<()> {
        let manager = manager()?;
        let session = manager
            .GetCurrentSession()
            .map_err(|_| AppError::media("No media player is currently playing."))?;

        let result = match command {
            MediaCommand::Play => session.TryPlayAsync(),
            MediaCommand::Pause => session.TryPauseAsync(),
            MediaCommand::Toggle => session.TryTogglePlayPauseAsync(),
            MediaCommand::Next => session.TrySkipNextAsync(),
            MediaCommand::Previous => session.TrySkipPreviousAsync(),
            MediaCommand::Stop => session.TryStopAsync(),
        };

        match result {
            Ok(op) => match op.join() {
                Ok(true) => Ok(()),
                // The player refused the command: it does not support it, or
                // it changed state between the check and the call.
                Ok(false) => Err(AppError::media(
                    "The player did not accept that command. It may not support it.",
                )),
                Err(e) => Err(AppError::media(format!("The player did not respond: {e}"))),
            },
            Err(e) => Err(AppError::media(format!("The player did not accept that command: {e}"))),
        }
    }

    pub fn seek(position_seconds: f64) -> AppResult<()> {
        let manager = manager()?;
        let session = manager
            .GetCurrentSession()
            .map_err(|_| AppError::media("No media player is currently playing."))?;

        let position = (position_seconds.max(0.0) * 10_000_000.0) as i64;
        match session.TryChangePlaybackPositionAsync(position) {
            Ok(op) => match op.join() {
                Ok(true) => Ok(()),
                Ok(false) => Err(AppError::media(
                    "This player does not allow seeking from another app.",
                )),
                Err(e) => Err(AppError::media(format!("The seek failed: {e}"))),
            },
            Err(e) => Err(AppError::media(format!("The seek failed: {e}"))),
        }
    }
}

/* -------------------------------------------------------------------------- */
/* Non-Windows stub                                                            */
/* -------------------------------------------------------------------------- */

#[cfg(not(windows))]
mod imp {
    use super::MediaService;
    use crate::error::{AppError, AppResult};
    use crate::types::{MediaCommand, MediaSession};

    pub fn current_session(_service: &MediaService) -> AppResult<Option<MediaSession>> {
        Ok(None)
    }

    pub fn send_command(_command: MediaCommand) -> AppResult<()> {
        Err(AppError::media("Media controls are only available on Windows."))
    }

    pub fn seek(_position_seconds: f64) -> AppResult<()> {
        Err(AppError::media("Seeking is only available on Windows."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(title: &str, position: f64) -> MediaSession {
        MediaSession {
            app_id: "Spotify".into(),
            title: Some(title.into()),
            artist: Some("Artist".into()),
            album: None,
            artwork: None,
            status: PlaybackStatus::Playing,
            position_seconds: position,
            duration_seconds: Some(180.0),
            can_play: true,
            can_pause: true,
            can_skip_next: true,
            can_skip_previous: true,
            can_seek: true,
        }
    }

    #[test]
    fn identical_snapshots_are_not_republished() {
        let a = session("Song", 12.0);
        let b = session("Song", 12.0);
        assert!(sessions_equal(Some(&a), Some(&b)));
    }

    #[test]
    fn sub_second_position_changes_are_ignored() {
        // Players report position continuously; a fraction of a second is not
        // a change the user can see.
        let a = session("Song", 12.1);
        let b = session("Song", 12.4);
        assert!(sessions_equal(Some(&a), Some(&b)));
    }

    #[test]
    fn a_whole_second_change_is_published() {
        let a = session("Song", 12.0);
        let b = session("Song", 13.0);
        assert!(!sessions_equal(Some(&a), Some(&b)));
    }

    #[test]
    fn a_track_change_is_published() {
        let a = session("Song A", 12.0);
        let b = session("Song B", 12.0);
        assert!(!sessions_equal(Some(&a), Some(&b)));
    }

    #[test]
    fn a_session_appearing_or_disappearing_is_published() {
        let a = session("Song", 12.0);
        assert!(!sessions_equal(None, Some(&a)));
        assert!(!sessions_equal(Some(&a), None));
        assert!(sessions_equal(None, None));
    }

    #[test]
    fn base64_matches_known_vectors() {
        // Vectors from RFC 4648.
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn image_types_are_detected_from_magic_bytes() {
        assert_eq!(sniff_image_type(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A]), Some("image/png"));
        assert_eq!(sniff_image_type(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff_image_type(b"not an image"), None);
    }
}
