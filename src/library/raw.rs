use std::collections::HashMap;

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Track {
    #[serde(rename = "Name")]
    pub name: String,

    #[serde(rename = "Location")]
    pub location: String,

    #[serde(rename = "Total Time")]
    pub duration_ms: u32,
}

#[derive(Deserialize)]
pub(super) struct TrackID { #[serde(rename="Track ID")] pub inner: u64 }

#[derive(Deserialize)]
pub(super) struct Playlist {
    #[serde(rename = "Playlist Persistent ID")]
    pub persistent_id: String,

    #[serde(rename = "Parent Persistent ID")]
    pub parent_id: Option<String>,

    #[serde(rename = "Name")]
    pub name: String,

    #[serde(rename = "Playlist Items", default)]
    pub items: Vec<TrackID>,

    #[serde(rename = "Folder", default)] // bool::default() returns false
    pub is_folder: bool,

    #[serde(rename = "Master", default)] // bool::default() returns false
    pub is_master: bool,

    #[serde(rename = "Distinguished Kind", default)]
    pub distinguished_kind: Option<u64>,
}

#[derive(Deserialize)]
pub struct Library {
    #[serde(rename = "Tracks")]
    pub(super) tracks: HashMap<String, Track>,

    #[serde(rename = "Playlists")]
    pub(super) playlists: Vec<Playlist>,
}