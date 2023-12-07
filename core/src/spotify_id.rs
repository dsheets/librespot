use std::{
    convert::TryFrom,
    fmt::{self, Write},
    str::{self, Split},
};

use data_encoding::HEXLOWER;
use percent_encoding::{percent_decode, utf8_percent_encode, AsciiSet, CONTROLS};
use thiserror::Error;

use crate::Error;

use librespot_protocol as protocol;

/// Types of basic Spotify items
///
/// Items of each of these types can be played but not all are
/// atomic. Each type of item is identified by a `SpotifyId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpotifyItemType {
    Album,
    Artist,
    Episode,
    Playlist,
    Show,
    Track,
}

impl TryFrom<&str> for SpotifyItemType {
    type Error = String;

    fn try_from(v: &str) -> Result<Self, String> {
        Ok(match v {
            "album" => Self::Album,
            "artist" => Self::Artist,
            "episode" => Self::Episode,
            "playlist" => Self::Playlist,
            "show" => Self::Show,
            "track" => Self::Track,
            _ => return Err(String::from(v)),
        })
    }
}

impl From<&SpotifyItemType> for &str {
    fn from(item_type: &SpotifyItemType) -> &'static str {
        match item_type {
            SpotifyItemType::Album => "album",
            SpotifyItemType::Artist => "artist",
            SpotifyItemType::Episode => "episode",
            SpotifyItemType::Playlist => "playlist",
            SpotifyItemType::Show => "show",
            SpotifyItemType::Track => "track",
        }
    }
}

impl fmt::Display for SpotifyItemType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.into())
    }
}

/// A 128-bit identifier for basic Spotify items
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpotifyId(u128);

impl SpotifyId {
    const SIZE: usize = 16;
    const SIZE_BASE16: usize = 32;
    const SIZE_BASE62: usize = 22;

    /// Parses a base16 (hex) encoded [Spotify ID] into a `SpotifyId`.
    ///
    /// `src` is expected to be 32 bytes long and encoded using valid characters.
    ///
    /// [Spotify ID]: https://developer.spotify.com/documentation/web-api/concepts/spotify-uris-ids
    pub fn from_base16(src: &str) -> Result<Self, SpotifyIdError> {
        let mut buf = [0u8; Self::SIZE];
        match HEXLOWER.decode_mut(src.as_ref(), &mut buf) {
            Ok(len) if len == Self::SIZE => Ok(Self(u128::from_be_bytes(buf))),
            Ok(_) => Err(SpotifyIdError::invalid_id_size(Self::SIZE_BASE16, src)),
            Err(e) => Err(SpotifyIdError::invalid_format_because(
                &format!("{}", e.error),
                src,
            )),
        }
    }

    /// Parses a base62 encoded [Spotify ID] into a `u128`.
    ///
    /// `src` is expected to be 22 bytes long and encoded using valid characters.
    ///
    /// [Spotify ID]: https://developer.spotify.com/documentation/web-api/concepts/spotify-uris-ids
    pub fn from_base62(src: &str) -> Result<Self, SpotifyIdError> {
        if src.len() != Self::SIZE_BASE62 {
            return Err(SpotifyIdError::invalid_id_size(Self::SIZE_BASE62, src));
        }

        match base62::decode_alternative(src) {
            Ok(x) => Ok(Self(x)),
            Err(e) => Err(SpotifyIdError::invalid_format_because(&format!("{e}"), src)),
        }
    }

    /// Creates a `u128` from a copy of `SpotifyId::SIZE` (16) bytes in big-endian order.
    ///
    /// The resulting `SpotifyId` will default to a `SpotifyItemType::Unknown`.
    pub fn from_buf(src: &[u8]) -> Result<Self, SpotifyIdError> {
        match src.try_into() {
            Ok(dst) => Ok(Self(u128::from_be_bytes(dst))),
            Err(_) => Err(SpotifyIdError::invalid_id_bytes(src)),
        }
    }

    /// Returns a copy of the `SpotifyId` as an array of `SpotifyId::SIZE` (16) bytes in
    /// big-endian order.
    pub fn into_buf(&self) -> [u8; Self::SIZE] {
        self.0.to_be_bytes()
    }

    /// Returns the `SpotifyId` as a base16 (hex) encoded, `SpotifyId::SIZE_BASE16` (32)
    /// character long `String`.
    pub fn into_base16(&self) -> String {
        HEXLOWER.encode(&u128::to_be_bytes(self.0))
    }

    /// Returns the `SpotifyId` as a [canonically] base62 encoded, `SpotifyId::SIZE_BASE62` (22)
    /// character long `String`.
    ///
    /// [canonically]: https://developer.spotify.com/documentation/web-api/concepts/spotify-uris-ids
    pub fn into_base62(&self) -> String {
        base62::encode_alternative(self.0)
    }
}

impl fmt::Debug for SpotifyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SpotifyId")
            .field(&self.into_base62())
            .finish()
    }
}

impl fmt::Display for SpotifyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.into_base62())
    }
}

/// A basic spotify item with type and identifier
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpotifyItem {
    item_type: SpotifyItemType,
    id: SpotifyId,
}

impl SpotifyItem {
    /// If we understand how to play this item as an individual unit.
    pub fn is_playable(&self) -> bool {
        match self.item_type {
            SpotifyItemType::Album
            | SpotifyItemType::Artist
            | SpotifyItemType::Playlist
            | SpotifyItemType::Show => false,
            SpotifyItemType::Episode | SpotifyItemType::Track => true,
        }
    }

    pub fn item_type(&self) -> SpotifyItemType {
        self.item_type
    }

    pub fn id(&self) -> SpotifyId {
        self.id
    }
}

impl From<&SpotifyItem> for String {
    fn from(value: &SpotifyItem) -> Self {
        format!("{value}")
    }
}

impl fmt::Display for &SpotifyItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("spotify:")?;
        f.write_str((&self.item_type).into())?;
        f.write_char(':')?;
        f.write_str(&self.id.into_base62())
    }
}

/// A Spotify meta data item
///
/// Currently, the only known metadata item type is `page` for
/// resolving pagination references
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpotifyMetaItem {
    Page(usize),
}

impl fmt::Display for SpotifyMetaItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpotifyMetaItem::Page(num) => {
                f.write_str("spotify:meta:page:")?;
                f.write_str(&num.to_string())
            }
        }
    }
}

impl From<&SpotifyMetaItem> for String {
    fn from(value: &SpotifyMetaItem) -> Self {
        format!("{value}")
    }
}

/// A Spotify local filesystem item
///
/// This is always a local music track with basic metadata. See [Spotify's local files documentation].
///
/// [Spotify's local files documentation]: https://developer.spotify.com/documentation/general/guides/local-files-spotify-playlists/
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpotifyLocalItem {
    artist: String,
    album_title: String,
    track_title: String,
    duration_s: u32,
}

impl SpotifyLocalItem {
    pub fn artist(&self) -> &str {
        &self.artist
    }

    pub fn album_title(&self) -> &str {
        &self.album_title
    }

    pub fn track_title(&self) -> &str {
        &self.track_title
    }

    pub fn duration_s(&self) -> u32 {
        self.duration_s
    }
}

fn url_decode(src: &str, s: &str) -> Result<String, SpotifyIdError> {
    // first, replace + with space
    let mut bytes = Vec::from(s.as_bytes());
    for b in bytes.iter_mut() {
        if *b == b'+' {
            *b = b' ';
        }
    }

    // then percent decode
    match percent_decode(&bytes).decode_utf8() {
        Ok(s) => Ok(String::from(s)),
        Err(e) => Err(SpotifyIdError::invalid_format_because(&format!("{e}"), src)),
    }
}

// space is not included as it is handled separately to/from '+'
const SPOTIFY_PCT_SET: &AsciiSet = &CONTROLS.add(b':').add(b'+').add(b'%');

fn url_encode(s: &str) -> String {
    // first percent encode
    let mut string = String::with_capacity(s.len());
    for utf8_seg in utf8_percent_encode(s, SPOTIFY_PCT_SET) {
        // then replace space with +
        for c in utf8_seg.chars() {
            string.push(match c {
                ' ' => '+',
                c => c,
            })
        }
    }

    string
}

impl fmt::Display for SpotifyLocalItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("spotify:local:")?;
        f.write_str(&url_encode(&self.artist))?;
        f.write_char(':')?;
        f.write_str(&url_encode(&self.album_title))?;
        f.write_char(':')?;
        f.write_str(&url_encode(&self.track_title))?;
        f.write_char(':')?;
        f.write_str(&self.duration_s.to_string())
    }
}

impl From<&SpotifyLocalItem> for String {
    fn from(value: &SpotifyLocalItem) -> Self {
        format!("{value}")
    }
}

/// Any Spotify URI with the 'spotify' scheme.
///
/// For example, `spotify:track:5sWHDYs0csV6RS48xBl0tH` could identify
/// a music track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpotifyUri {
    Item(SpotifyItem),
    UserItem(String, SpotifyItem),
    Station(SpotifyItem),
    Meta(SpotifyMetaItem),
    Local(SpotifyLocalItem),
    Unknown(String, Option<String>),
}

/// Errors that can occur when processing Spotify URIs or Spotify IDs
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SpotifyIdError {
    #[error("ID '{1}' cannot be parsed: wrong identifier size; expected {0} was {}", .1.len())]
    InvalidIdSize(usize, String),
    #[error("ID bytes '{0:?}' cannot be parsed")]
    InvalidIdBytes(Vec<u8>),
    #[error("'{1}' is not a valid Spotify URI: {0}")]
    InvalidFormat(String, String),
    #[error("URI '{0}' does not have the 'spotify' scheme")]
    InvalidScheme(String),
}

impl SpotifyIdError {
    fn invalid_id_size(k: usize, s: &str) -> Self {
        Self::InvalidIdSize(k, String::from(s))
    }

    fn invalid_id_bytes(b: &[u8]) -> Self {
        Self::InvalidIdBytes(Vec::from(b))
    }

    fn invalid_format_because(reason: &str, s: &str) -> Self {
        Self::InvalidFormat(String::from(reason), String::from(s))
    }

    fn invalid_scheme(s: &str) -> Self {
        Self::InvalidScheme(String::from(s))
    }
}

impl From<SpotifyIdError> for Error {
    fn from(err: SpotifyIdError) -> Self {
        Error::invalid_argument(err)
    }
}

impl TryFrom<&str> for SpotifyUri {
    type Error = SpotifyIdError;
    fn try_from(src: &str) -> Result<Self, Self::Error> {
        let mut parts = src.split(':');

        match Self::next_str_from_split(src, &mut parts)? {
            "spotify" => (),
            _ => return Err(SpotifyIdError::invalid_scheme(src)),
        }

        match Self::next_str_from_split(src, &mut parts)? {
            "user" => {
                let user = Self::next_str_from_split(src, &mut parts)?;
                Self::from_src_parts_inj(
                    src,
                    &mut parts,
                    |item| Self::UserItem(String::from(user), item),
                    |other, rest| {
                        let rest = match rest {
                            Some(rest) => format!(":{rest}"),
                            None => "".to_string(),
                        };
                        Self::Unknown(String::from("user"), Some(format!("{user}:{other}{rest}")))
                    },
                )
            }
            "station" => Self::from_src_parts_inj(src, &mut parts, Self::Station, |other, rest| {
                let rest = match rest {
                    Some(rest) => format!(":{rest}"),
                    None => "".to_string(),
                };
                Self::Unknown(String::from("station"), Some(other + &rest))
            }),
            "meta" => Self::meta_from_src_parts(src, &mut parts),
            "local" => Self::local_from_src_parts(src, &mut parts),
            other => {
                Self::from_src_typ_parts_inj(src, other, &mut parts, Self::Item, Self::Unknown)
            }
        }
    }
}

impl SpotifyUri {
    pub fn track(id: SpotifyId) -> Self {
        Self::Item(SpotifyItem {
            item_type: SpotifyItemType::Track,
            id,
        })
    }

    pub fn album(id: SpotifyId) -> Self {
        Self::Item(SpotifyItem {
            item_type: SpotifyItemType::Album,
            id,
        })
    }

    pub fn artist(id: SpotifyId) -> Self {
        Self::Item(SpotifyItem {
            item_type: SpotifyItemType::Artist,
            id,
        })
    }

    pub fn episode(id: SpotifyId) -> Self {
        Self::Item(SpotifyItem {
            item_type: SpotifyItemType::Episode,
            id,
        })
    }

    pub fn playlist(id: SpotifyId) -> Self {
        Self::Item(SpotifyItem {
            item_type: SpotifyItemType::Playlist,
            id,
        })
    }

    pub fn show(id: SpotifyId) -> Self {
        Self::Item(SpotifyItem {
            item_type: SpotifyItemType::Show,
            id,
        })
    }

    pub fn item(&self) -> Option<&SpotifyItem> {
        match self {
            SpotifyUri::Item(item) | SpotifyUri::UserItem(_, item) => Some(item),
            SpotifyUri::Station(_) // this does not identify an item but rather a (recommendation) function of an item
            | SpotifyUri::Meta(_)
            | SpotifyUri::Local(_)
            | SpotifyUri::Unknown(_, _) => None,
        }
    }

    pub fn id(&self) -> Option<SpotifyId> {
        self.item().map(|i| i.id)
    }

    pub fn item_type(&self) -> Option<SpotifyItemType> {
        self.item().map(|i| i.item_type)
    }

    pub fn username(&self) -> Option<&str> {
        match self {
            SpotifyUri::UserItem(username, _) => Some(username),
            SpotifyUri::Item(_)
            | SpotifyUri::Station(_)
            | SpotifyUri::Meta(_)
            | SpotifyUri::Local(_)
            | SpotifyUri::Unknown(_, _) => None,
        }
    }

    /// If we understand how to play this URI as an individual unit.
    pub fn is_playable(&self) -> bool {
        match self {
            SpotifyUri::Item(item) => item.is_playable(),
            SpotifyUri::UserItem(_, item) => item.is_playable(),
            SpotifyUri::Station(_)
            | SpotifyUri::Meta(_)
            | SpotifyUri::Local(_)
            | SpotifyUri::Unknown(_, _) => false,
        }
    }

    fn next_str_from_split<'a>(
        src: &'a str,
        parts: &mut Split<'a, char>,
    ) -> Result<&'a str, SpotifyIdError> {
        parts
            .next()
            .ok_or_else(|| SpotifyIdError::invalid_format_because("missing part", src))
    }

    fn from_src_parts_inj<
        'a,
        INJ1: FnOnce(SpotifyItem) -> Self,
        INJ2: FnOnce(String, Option<String>) -> Self,
    >(
        src: &'a str,
        parts: &mut Split<'a, char>,
        inj1: INJ1,
        inj2: INJ2,
    ) -> Result<Self, SpotifyIdError> {
        let typ = Self::next_str_from_split(src, parts)?;
        Self::from_src_typ_parts_inj(src, typ, parts, inj1, inj2)
    }

    fn rest_from_parts(parts: &mut Split<'_, char>) -> Option<String> {
        parts.next().map(|next| {
            let mut s = String::from(next);
            for part in parts {
                s.push(':');
                s.push_str(part);
            }
            s
        })
    }

    fn str_rest_from_parts(parts: &mut Split<'_, char>) -> String {
        match Self::rest_from_parts(parts) {
            Some(s) => format!(":{s}"),
            None => "".to_string(),
        }
    }

    fn from_src_typ_parts_inj<
        'a,
        INJ1: FnOnce(SpotifyItem) -> Self,
        INJ2: FnOnce(String, Option<String>) -> Self,
    >(
        src: &'a str,
        typ: &str,
        parts: &mut Split<'a, char>,
        inj1: INJ1,
        inj2: INJ2,
    ) -> Result<Self, SpotifyIdError> {
        match typ.try_into() {
            Ok(item_type) => {
                let id_str = Self::next_str_from_split(src, parts)?;
                let id = SpotifyId::from_base62(id_str)?;
                match parts.next() {
                    Some(next) => Ok(inj2(
                        String::from(typ),
                        Some(format!(
                            "{id_str}:{next}{}",
                            Self::str_rest_from_parts(parts)
                        )),
                    )),
                    None => Ok(inj1(SpotifyItem { item_type, id })),
                }
            }
            Err(other) => Ok(inj2(other, Self::rest_from_parts(parts))),
        }
    }

    fn meta_from_src_parts<'a>(
        src: &'a str,
        parts: &mut Split<'a, char>,
    ) -> Result<Self, SpotifyIdError> {
        match Self::next_str_from_split(src, parts)? {
            "page" => match Self::next_str_from_split(src, parts)?.parse() {
                Ok(n) => match parts.next() {
                    None => Ok(Self::Meta(SpotifyMetaItem::Page(n))),
                    Some(next) => Ok(Self::Unknown(
                        String::from("meta"),
                        Some(format!(
                            "page:{n}:{next}{}",
                            Self::str_rest_from_parts(parts)
                        )),
                    )),
                },
                Err(e) => Err(SpotifyIdError::invalid_format_because(&format!("{e}"), src)),
            },
            other => Ok(Self::Unknown(
                String::from("meta"),
                Some(format!("{other}{}", Self::str_rest_from_parts(parts),)),
            )),
        }
    }

    fn local_from_src_parts<'a>(
        src: &'a str,
        parts: &mut Split<'a, char>,
    ) -> Result<Self, SpotifyIdError> {
        let artist = Self::next_str_from_split(src, parts)?;
        let album_title = Self::next_str_from_split(src, parts)?;
        let track_title = Self::next_str_from_split(src, parts)?;
        let duration_s = Self::next_str_from_split(src, parts)?;
        match parts.next() {
            None => Ok(Self::Local(SpotifyLocalItem {
                artist: url_decode(src, artist)?,
                album_title: url_decode(src, album_title)?,
                track_title: url_decode(src, track_title)?,
                duration_s: duration_s
                    .parse::<u32>()
                    .map_err(|e| SpotifyIdError::invalid_format_because(&format!("{e}"), src))?,
            })),
            Some(next) => Ok(Self::Unknown(
                String::from("local"),
                Some(format!(
                    "{artist}:{album_title}:{track_title}:{duration_s}:{next}{}",
                    Self::str_rest_from_parts(parts),
                )),
            )),
        }
    }
}

impl fmt::Display for SpotifyUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpotifyUri::Item(item) => item.fmt(f),
            SpotifyUri::UserItem(user, item) => {
                f.write_str("spotify:user:")?;
                f.write_str(user)?;
                f.write_char(':')?;
                item.item_type.fmt(f)?;
                f.write_char(':')?;
                f.write_str(&item.id.into_base62())
            }
            SpotifyUri::Station(item) => {
                f.write_str("spotify:station:")?;
                item.item_type.fmt(f)?;
                f.write_char(':')?;
                f.write_str(&item.id.into_base62())
            }
            SpotifyUri::Meta(meta) => meta.fmt(f),
            SpotifyUri::Local(local) => local.fmt(f),
            SpotifyUri::Unknown(typ, rest) => {
                f.write_str("spotify:")?;
                f.write_str(typ)?;
                if let Some(rest) = rest {
                    f.write_char(':')?;
                    f.write_str(rest)
                } else {
                    Ok(())
                }
            }
        }
    }
}

impl From<&SpotifyUri> for String {
    fn from(value: &SpotifyUri) -> Self {
        format!("{value}")
    }
}

impl TryFrom<&[u8]> for SpotifyId {
    type Error = SpotifyIdError;
    fn try_from(src: &[u8]) -> Result<Self, Self::Error> {
        Self::from_buf(src)
    }
}

impl TryFrom<&str> for SpotifyId {
    type Error = SpotifyIdError;
    fn try_from(src: &str) -> Result<Self, Self::Error> {
        Self::from_base62(src)
    }
}

impl TryFrom<&Vec<u8>> for SpotifyId {
    type Error = SpotifyIdError;
    fn try_from(src: &Vec<u8>) -> Result<Self, Self::Error> {
        Self::try_from(src.as_slice())
    }
}

impl TryFrom<&protocol::spirc::TrackRef> for SpotifyUri {
    type Error = crate::Error;
    fn try_from(track: &protocol::spirc::TrackRef) -> Result<Self, Self::Error> {
        match SpotifyId::from_buf(track.gid()) {
            Ok(id) => Ok(Self::track(id)),
            Err(_) => Ok(Self::try_from(track.uri())?),
        }
    }
}

impl TryFrom<&protocol::metadata::Album> for SpotifyUri {
    type Error = crate::Error;
    fn try_from(album: &protocol::metadata::Album) -> Result<Self, Self::Error> {
        Ok(Self::album(SpotifyId::from_buf(album.gid())?))
    }
}

impl TryFrom<&protocol::metadata::Artist> for SpotifyUri {
    type Error = crate::Error;
    fn try_from(artist: &protocol::metadata::Artist) -> Result<Self, Self::Error> {
        Ok(Self::artist(SpotifyId::from_buf(artist.gid())?))
    }
}

impl TryFrom<&protocol::metadata::Episode> for SpotifyUri {
    type Error = crate::Error;
    fn try_from(episode: &protocol::metadata::Episode) -> Result<Self, Self::Error> {
        Ok(Self::episode(SpotifyId::from_buf(episode.gid())?))
    }
}

impl TryFrom<&protocol::metadata::Track> for SpotifyUri {
    type Error = crate::Error;
    fn try_from(track: &protocol::metadata::Track) -> Result<Self, Self::Error> {
        Ok(Self::track(SpotifyId::from_buf(track.gid())?))
    }
}

impl TryFrom<&protocol::metadata::Show> for SpotifyUri {
    type Error = crate::Error;
    fn try_from(show: &protocol::metadata::Show) -> Result<Self, Self::Error> {
        Ok(Self::show(SpotifyId::from_buf(show.gid())?))
    }
}

impl TryFrom<&protocol::metadata::ArtistWithRole> for SpotifyUri {
    type Error = crate::Error;
    fn try_from(artist: &protocol::metadata::ArtistWithRole) -> Result<Self, Self::Error> {
        Ok(Self::artist(SpotifyId::from_buf(artist.artist_gid())?))
    }
}

impl TryFrom<&protocol::playlist4_external::Item> for SpotifyUri {
    type Error = crate::Error;
    fn try_from(item: &protocol::playlist4_external::Item) -> Result<Self, Self::Error> {
        Ok(Self::try_from(item.uri())?)
    }
}

// Note that this is the unique revision of an item's metadata on a playlist,
// not the ID of that item or playlist.
impl TryFrom<&protocol::playlist4_external::MetaItem> for SpotifyId {
    type Error = crate::Error;
    fn try_from(item: &protocol::playlist4_external::MetaItem) -> Result<Self, Self::Error> {
        Ok(Self::try_from(item.revision())?)
    }
}

// Note that this is the unique revision of a playlist, not the ID of that playlist.
impl TryFrom<&protocol::playlist4_external::SelectedListContent> for SpotifyId {
    type Error = crate::Error;
    fn try_from(
        playlist: &protocol::playlist4_external::SelectedListContent,
    ) -> Result<Self, Self::Error> {
        Ok(Self::try_from(playlist.revision())?)
    }
}

// TODO: check meaning and format of this field in the wild. [old:
// This might be a FileId, which is why we now don't create a separate
// `Playlist` enum value yet and choose to discard any item type.]
impl TryFrom<&protocol::playlist_annotate3::TranscodedPicture> for SpotifyUri {
    type Error = crate::Error;
    fn try_from(
        picture: &protocol::playlist_annotate3::TranscodedPicture,
    ) -> Result<Self, Self::Error> {
        Ok(Self::try_from(picture.uri())?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ItemConversionCase {
        id: u128,
        kind: SpotifyItemType,
        uri: &'static str,
        base16: &'static str,
        base62: &'static str,
        raw: &'static [u8],
    }

    static ITEM_CONV_VALID: [ItemConversionCase; 7] = [
        ItemConversionCase {
            id: 238762092608182713602505436543891614649,
            kind: SpotifyItemType::Track,
            uri: "spotify:track:5sWHDYs0csV6RS48xBl0tH",
            base16: "b39fe8081e1f4c54be38e8d6f9f12bb9",
            base62: "5sWHDYs0csV6RS48xBl0tH",
            raw: &[
                179, 159, 232, 8, 30, 31, 76, 84, 190, 56, 232, 214, 249, 241, 43, 185,
            ],
        },
        ItemConversionCase {
            id: 204841891221366092811751085145916697048,
            kind: SpotifyItemType::Track,
            uri: "spotify:track:4GNcXTGWmnZ3ySrqvol3o4",
            base16: "9a1b1cfbc6f244569ae0356c77bbe9d8",
            base62: "4GNcXTGWmnZ3ySrqvol3o4",
            raw: &[
                154, 27, 28, 251, 198, 242, 68, 86, 154, 224, 53, 108, 119, 187, 233, 216,
            ],
        },
        ItemConversionCase {
            id: 204841891221366092811751085145916697048,
            kind: SpotifyItemType::Episode,
            uri: "spotify:episode:4GNcXTGWmnZ3ySrqvol3o4",
            base16: "9a1b1cfbc6f244569ae0356c77bbe9d8",
            base62: "4GNcXTGWmnZ3ySrqvol3o4",
            raw: &[
                154, 27, 28, 251, 198, 242, 68, 86, 154, 224, 53, 108, 119, 187, 233, 216,
            ],
        },
        ItemConversionCase {
            id: 204841891221366092811751085145916697048,
            kind: SpotifyItemType::Show,
            uri: "spotify:show:4GNcXTGWmnZ3ySrqvol3o4",
            base16: "9a1b1cfbc6f244569ae0356c77bbe9d8",
            base62: "4GNcXTGWmnZ3ySrqvol3o4",
            raw: &[
                154, 27, 28, 251, 198, 242, 68, 86, 154, 224, 53, 108, 119, 187, 233, 216,
            ],
        },
        ItemConversionCase {
            id: 204841891221366092811751085145916697048,
            kind: SpotifyItemType::Playlist,
            uri: "spotify:playlist:4GNcXTGWmnZ3ySrqvol3o4",
            base16: "9a1b1cfbc6f244569ae0356c77bbe9d8",
            base62: "4GNcXTGWmnZ3ySrqvol3o4",
            raw: &[
                154, 27, 28, 251, 198, 242, 68, 86, 154, 224, 53, 108, 119, 187, 233, 216,
            ],
        },
        ItemConversionCase {
            id: 204841891221366092811751085145916697048,
            kind: SpotifyItemType::Artist,
            uri: "spotify:artist:4GNcXTGWmnZ3ySrqvol3o4",
            base16: "9a1b1cfbc6f244569ae0356c77bbe9d8",
            base62: "4GNcXTGWmnZ3ySrqvol3o4",
            raw: &[
                154, 27, 28, 251, 198, 242, 68, 86, 154, 224, 53, 108, 119, 187, 233, 216,
            ],
        },
        ItemConversionCase {
            id: 204841891221366092811751085145916697048,
            kind: SpotifyItemType::Album,
            uri: "spotify:album:4GNcXTGWmnZ3ySrqvol3o4",
            base16: "9a1b1cfbc6f244569ae0356c77bbe9d8",
            base62: "4GNcXTGWmnZ3ySrqvol3o4",
            raw: &[
                154, 27, 28, 251, 198, 242, 68, 86, 154, 224, 53, 108, 119, 187, 233, 216,
            ],
        },
    ];

    #[test]
    fn from_base62() {
        for c in &ITEM_CONV_VALID {
            assert_eq!(SpotifyId::from_base62(c.base62).unwrap().0, c.id);
        }
    }

    #[test]
    fn into_base62() {
        for c in &ITEM_CONV_VALID {
            let item = SpotifyItem {
                id: SpotifyId(c.id),
                item_type: c.kind,
            };

            assert_eq!(item.id.into_base62(), c.base62);
        }
    }

    #[test]
    fn from_base16() {
        for c in &ITEM_CONV_VALID {
            assert_eq!(SpotifyId::from_base16(c.base16).unwrap().0, c.id);
        }
    }

    #[test]
    fn into_base16() {
        for c in &ITEM_CONV_VALID {
            let item = SpotifyItem {
                id: SpotifyId(c.id),
                item_type: c.kind,
            };

            assert_eq!(item.id.into_base16(), c.base16);
        }
    }

    #[test]
    fn from_uri() {
        for c in &ITEM_CONV_VALID {
            let actual = SpotifyUri::try_from(c.uri).unwrap();

            assert_eq!(actual.id().map(|x| x.0), Some(c.id));
            assert_eq!(actual.item_type(), Some(c.kind));
        }
    }

    #[test]
    fn from_uri_id_short() {
        assert!(SpotifyUri::try_from("spotify:album:4GNcXTGWmnZ3ySrqvol3o").is_err())
    }

    #[test]
    fn from_uri_id_long() {
        assert!(SpotifyUri::try_from("spotify:album:4GNcXTGWmnZ3ySrqvol3o45").is_err())
    }

    #[test]
    fn from_uri_id_bad_char() {
        assert!(SpotifyUri::try_from("spotify:album:4GNcXTGWmnZ3ySrqvol3o%").is_err())
    }

    #[test]
    fn from_local_uri() {
        let uri = SpotifyUri::try_from("spotify:local:abc:ghi:xyz:123").unwrap();

        if let SpotifyUri::Local(SpotifyLocalItem {
            artist,
            album_title,
            track_title,
            duration_s,
        }) = uri
        {
            assert_eq!(artist, "abc");
            assert_eq!(album_title, "ghi");
            assert_eq!(track_title, "xyz");
            assert_eq!(duration_s, 123);
        } else {
            panic!("should parse as local URI");
        }
    }

    #[test]
    fn from_local_uri_short() {
        assert!(SpotifyUri::try_from("spotify:local").is_err());
        assert!(SpotifyUri::try_from("spotify:local:artist").is_err());
        assert!(SpotifyUri::try_from("spotify:local:artist:album").is_err());
        assert!(SpotifyUri::try_from("spotify:local:artist:album:track").is_err());
    }

    #[test]
    fn from_local_uri_long() {
        assert_eq!(
            SpotifyUri::try_from("spotify:local:artist:album:track:123:").unwrap(),
            SpotifyUri::Unknown(
                "local".to_string(),
                Some("artist:album:track:123:".to_string())
            )
        );
        assert_eq!(
            SpotifyUri::try_from("spotify:local:artist:album:track:123:a:b:c").unwrap(),
            SpotifyUri::Unknown(
                "local".to_string(),
                Some("artist:album:track:123:a:b:c".to_string())
            )
        )
    }

    #[test]
    fn from_local_uri_bad_duration() {
        assert!(SpotifyUri::try_from("spotify:local:artist:album:track:").is_err());
        assert!(SpotifyUri::try_from("spotify:local:artist:album:track:a").is_err());
        assert!(SpotifyUri::try_from("spotify:local:artist:album:track:1.").is_err());
        assert!(SpotifyUri::try_from(
            "spotify:local:artist:album:track:99999999999999999999999999999999999999999999999"
        )
        .is_err());
    }

    #[test]
    fn from_local_uri_pct() {
        let uri_unnorm = "spotify:local:Artist+Name:Album%3a%20Subtitle:Track#:120";
        let uri_norm = "spotify:local:Artist+Name:Album%3A+Subtitle:Track#:120";
        let local = SpotifyLocalItem {
            artist: "Artist Name".to_string(),
            album_title: "Album: Subtitle".to_string(),
            track_title: "Track#".to_string(),
            duration_s: 120,
        };
        assert_eq!(
            SpotifyUri::try_from(uri_unnorm).unwrap(),
            SpotifyUri::Local(local.clone())
        );
        assert_eq!(SpotifyUri::Local(local).to_string(), uri_norm)
    }

    #[test]
    fn from_user_uri() {
        let actual =
            SpotifyUri::try_from("spotify:user:name:playlist:37i9dQZF1DWSw8liJZcPOI").unwrap();

        assert_eq!(
            actual.id().map(|x| x.0),
            Some(136159921382084734723401526672209703396)
        );
        assert_eq!(actual.item_type(), Some(SpotifyItemType::Playlist));
        assert_eq!(actual.username(), Some("name"));
    }

    #[test]
    fn from_user_uri_short() {
        assert!(SpotifyUri::try_from("spotify:user").is_err());
        assert!(SpotifyUri::try_from("spotify:user:name").is_err());
        assert!(SpotifyUri::try_from("spotify:user:name:track").is_err());
    }

    #[test]
    fn from_user_uri_long() {
        assert_eq!(
            SpotifyUri::try_from("spotify:user:name:track:37i9dQZF1DWSw8liJZcPOI:more").unwrap(),
            SpotifyUri::Unknown(
                "user".to_string(),
                Some("name:track:37i9dQZF1DWSw8liJZcPOI:more".to_string())
            )
        )
    }

    #[test]
    fn from_user_uri_unknown() {
        assert_eq!(
            SpotifyUri::try_from("spotify:user:name:unicorn").unwrap(),
            SpotifyUri::Unknown("user".to_string(), Some("name:unicorn".to_string()),)
        );
        assert_eq!(
            SpotifyUri::try_from("spotify:user:name:unicorn:").unwrap(),
            SpotifyUri::Unknown("user".to_string(), Some("name:unicorn:".to_string()),)
        );
        assert_eq!(
            SpotifyUri::try_from("spotify:user:name:unicorn::").unwrap(),
            SpotifyUri::Unknown("user".to_string(), Some("name:unicorn::".to_string()),)
        )
    }

    #[test]
    fn from_station_uri() {
        let uri = SpotifyUri::try_from("spotify:station:track:37i9dQZF1DWSw8liJZcPOI").unwrap();

        assert_eq!(
            uri,
            SpotifyUri::Station(SpotifyItem {
                item_type: SpotifyItemType::Track,
                id: SpotifyId::from_base62("37i9dQZF1DWSw8liJZcPOI").unwrap()
            })
        )
    }

    #[test]
    fn from_station_uri_short() {
        assert!(SpotifyUri::try_from("spotify:station").is_err());
        assert!(SpotifyUri::try_from("spotify:station:track").is_err());
    }

    #[test]
    fn from_station_uri_long() {
        assert_eq!(
            SpotifyUri::try_from("spotify:station:track:37i9dQZF1DWSw8liJZcPOI:more").unwrap(),
            SpotifyUri::Unknown(
                "station".to_string(),
                Some("track:37i9dQZF1DWSw8liJZcPOI:more".to_string())
            )
        )
    }

    #[test]
    fn from_station_uri_unknown() {
        assert_eq!(
            SpotifyUri::try_from("spotify:station:typ").unwrap(),
            SpotifyUri::Unknown("station".to_string(), Some("typ".to_string()))
        );
        assert_eq!(
            SpotifyUri::try_from("spotify:station:typ:a").unwrap(),
            SpotifyUri::Unknown("station".to_string(), Some("typ:a".to_string()))
        );
        assert_eq!(
            SpotifyUri::try_from("spotify:station:typ:a:b").unwrap(),
            SpotifyUri::Unknown("station".to_string(), Some("typ:a:b".to_string()))
        )
    }

    #[test]
    fn from_meta_uri() {
        let uri = SpotifyUri::try_from("spotify:meta:page:2").unwrap();

        assert_eq!(uri, SpotifyUri::Meta(SpotifyMetaItem::Page(2)))
    }

    #[test]
    fn from_meta_uri_short() {
        assert!(SpotifyUri::try_from("spotify:meta").is_err());
        assert!(SpotifyUri::try_from("spotify:meta:page").is_err());
    }

    #[test]
    fn from_meta_uri_long() {
        assert_eq!(
            SpotifyUri::try_from("spotify:meta:page:2:").unwrap(),
            SpotifyUri::Unknown("meta".to_string(), Some("page:2:".to_string()))
        );
        assert_eq!(
            SpotifyUri::try_from("spotify:meta:page:2:more").unwrap(),
            SpotifyUri::Unknown("meta".to_string(), Some("page:2:more".to_string()))
        );
    }

    #[test]
    fn from_meta_uri_unknown() {
        assert_eq!(
            SpotifyUri::try_from("spotify:meta:idea").unwrap(),
            SpotifyUri::Unknown("meta".to_string(), Some("idea".to_string()))
        );
        assert_eq!(
            SpotifyUri::try_from("spotify:meta:idea:1").unwrap(),
            SpotifyUri::Unknown("meta".to_string(), Some("idea:1".to_string()))
        );
        assert_eq!(
            SpotifyUri::try_from("spotify:meta:idea:1:2").unwrap(),
            SpotifyUri::Unknown("meta".to_string(), Some("idea:1:2".to_string()))
        );
    }

    #[test]
    fn from_meta_uri_bad_page() {
        assert!(SpotifyUri::try_from("spotify:meta:page:").is_err());
        assert!(SpotifyUri::try_from("spotify:meta:page:a").is_err());
        assert!(SpotifyUri::try_from("spotify:meta:page:1.").is_err());
        assert!(SpotifyUri::try_from("spotify:meta:page:99999999999999999999999999999").is_err());
    }

    #[test]
    fn from_unknown() {
        assert_eq!(
            SpotifyUri::try_from("spotify:unicorn").unwrap(),
            SpotifyUri::Unknown("unicorn".to_string(), None)
        );
        assert_eq!(
            SpotifyUri::try_from("spotify:unicorn:").unwrap(),
            SpotifyUri::Unknown("unicorn".to_string(), Some("".to_string()))
        );
        assert_eq!(
            SpotifyUri::try_from("spotify:unicorn::").unwrap(),
            SpotifyUri::Unknown("unicorn".to_string(), Some(":".to_string()))
        );
    }

    #[test]
    fn from_bad_scheme() {
        let url = "http://example.net/";
        assert_eq!(
            SpotifyUri::try_from(url).unwrap_err(),
            SpotifyIdError::InvalidScheme(url.to_string())
        )
    }

    #[test]
    fn from_buf() {
        for c in &ITEM_CONV_VALID {
            assert_eq!(SpotifyId::from_buf(c.raw).unwrap().0, c.id);
        }
    }

    #[test]
    fn from_buf_invalid() {
        assert!(SpotifyId::from_buf(&[]).is_err());
        assert!(SpotifyId::from_buf(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]).is_err());
        assert!(SpotifyId::from_buf(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]).is_err());
    }

    #[test]
    fn to_uri() {
        for c in &ITEM_CONV_VALID {
            let item = SpotifyItem {
                id: SpotifyId(c.id),
                item_type: c.kind,
            };

            assert_eq!(String::from(&SpotifyUri::Item(item)), c.uri);
        }
    }

    #[test]
    fn to_user_uri() {
        assert_eq!(
            SpotifyUri::UserItem(
                "name".to_string(),
                SpotifyItem {
                    item_type: SpotifyItemType::Track,
                    id: SpotifyId::from_base62("37i9dQZF1DWSw8liJZcPOI").unwrap(),
                }
            )
            .to_string(),
            "spotify:user:name:track:37i9dQZF1DWSw8liJZcPOI".to_string()
        )
    }

    #[test]
    fn to_local_uri() {
        assert_eq!(
            SpotifyUri::Local(SpotifyLocalItem {
                artist: "artist".to_string(),
                album_title: "album".to_string(),
                track_title: "track".to_string(),
                duration_s: 120,
            })
            .to_string(),
            "spotify:local:artist:album:track:120".to_string()
        )
    }

    #[test]
    fn to_meta_uri() {
        assert_eq!(
            SpotifyUri::Meta(SpotifyMetaItem::Page(2)).to_string(),
            "spotify:meta:page:2".to_string()
        )
    }

    #[test]
    fn to_station_uri() {
        assert_eq!(
            SpotifyUri::Station(SpotifyItem {
                item_type: SpotifyItemType::Track,
                id: SpotifyId::from_base62("37i9dQZF1DWSw8liJZcPOI").unwrap(),
            })
            .to_string(),
            "spotify:station:track:37i9dQZF1DWSw8liJZcPOI".to_string()
        )
    }

    #[test]
    fn to_unknown_uri() {
        assert_eq!(
            SpotifyUri::Unknown("unicorn".to_string(), Some("more:::".to_string())).to_string(),
            "spotify:unicorn:more:::".to_string()
        )
    }
}
