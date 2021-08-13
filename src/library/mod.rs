use std::collections::HashMap;

mod raw;
pub use raw::Track;

#[derive(Debug)]
pub enum ParseError {
    InvalidPlaylistId(std::num::ParseIntError),
    InvalidTrackId(std::num::ParseIntError),
    NonUtf8Path(std::str::Utf8Error),
}

#[derive(Debug)]
pub enum PlaylistKind {
    Master,
    Folder,
    Generic,
    Unknown(u64),
}

pub struct Playlist {
    pub persistent_id: u64,
    pub parent_id: Option<u64>,
    pub name: String,
    pub kind: PlaylistKind,
    pub items: Vec<u64>,
    pub order_key: usize,
}

impl Playlist {
    fn from_raw(raw: raw::Playlist, order_key: usize) -> Result<Playlist, ParseError> {
        fn playlist_kind(raw: &raw::Playlist) -> PlaylistKind {
            if raw.is_master { return PlaylistKind::Master; }
            if raw.is_folder { return PlaylistKind::Folder; }
            match raw.distinguished_kind {
                None => PlaylistKind::Generic,
                Some(other) => PlaylistKind::Unknown(other),
            }
        }

        Ok(Playlist {
            kind: playlist_kind(&raw),
            persistent_id: u64::from_str_radix(&raw.persistent_id, 16)
                .map_err(ParseError::InvalidPlaylistId)?,
            parent_id: raw.parent_id
                .map(|id| u64::from_str_radix(&id, 16))
                .transpose()
                .map_err(ParseError::InvalidPlaylistId)?,
            name: raw.name,
            items: raw.items.iter().map(|id| id.inner).collect(),
            order_key
        })
    }
}

pub struct Library {
    pub tracks: HashMap<u64, Track>,
    pub playlists: HashMap<u64, Playlist>,
    playlist_index: Vec<(Option<u64>, u64)>,
}

impl Library {
    pub fn visit_playlists(&self, mut visitor: impl FnMut(u64, usize)) {
        self.visit_playlists_inner(None, 0, &mut visitor);
    }

    fn visit_playlists_inner(&self, node_id: Option<u64>, depth: usize, visitor: &mut impl FnMut(u64, usize)) {
        let child = self.playlist_index.binary_search_by_key(&node_id, |(parent, _)| *parent);
        if let Ok(mut child) = child {
            while child > 0 && self.playlist_index[child - 1].0 == node_id { child -= 1; }
            while child < self.playlist_index.len() && self.playlist_index[child].0 == node_id {
                let child_id = self.playlist_index[child].1;
                visitor(child_id, depth);
                self.visit_playlists_inner(Some(child_id), depth + 1, visitor);
                child += 1;
            }
        }
    }

    pub fn from_raw(raw: raw::Library) -> Result<Library, ParseError> {
        let playlist_count = raw.playlists.len();
        let playlists = raw.playlists.into_iter().enumerate().try_fold(
            HashMap::with_capacity(playlist_count), 
            |mut playlists, (order_key, playlist)| {
                let playlist = Playlist::from_raw(playlist, order_key)?;
                playlists.insert(playlist.persistent_id, playlist);
                Ok(playlists)
            }
        )?;
        
        let mut playlist_index: Vec<_> = playlists.values()
            .map(|p| (p.parent_id, p.persistent_id))
            .collect();

        playlist_index.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| {
            playlists[&a.1].order_key.cmp(&playlists[&b.1].order_key)
        }));

        let tracks: Result<HashMap<_, _>, ParseError> = raw.tracks.into_iter()
            .map(|(id, track)| match u64::from_str_radix(&id, 10) {
                Ok(id) => Ok((id, process_track(track)?)),
                Err(e) => Err(ParseError::InvalidTrackId(e)),
            }).collect();

        Ok(Library { tracks: tracks?, playlists, playlist_index })
    }
}

fn process_track(track: Track) -> Result<Track, ParseError> {
    let location = percent_encoding::percent_decode_str(&track.location)
        .decode_utf8()
        .map_err(ParseError::NonUtf8Path)?
        .into_owned();
    Ok(Track { location, .. track })
}