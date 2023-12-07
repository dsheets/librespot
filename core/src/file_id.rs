use std::fmt;

use data_encoding::HEXLOWER;

use librespot_protocol as protocol;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(pub [u8; 20]);

impl FileId {
    pub fn from_raw(src: &[u8]) -> FileId {
        let mut dst = [0u8; 20];
        dst.clone_from_slice(src);
        FileId(dst)
    }

    pub fn into_base16(&self) -> String {
        HEXLOWER.encode(&self.0)
    }
}

impl fmt::Debug for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("FileId").field(&self.into_base16()).finish()
    }
}

impl fmt::Display for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.into_base16())
    }
}

impl From<&[u8]> for FileId {
    fn from(src: &[u8]) -> Self {
        Self::from_raw(src)
    }
}
impl From<&protocol::metadata::Image> for FileId {
    fn from(image: &protocol::metadata::Image) -> Self {
        Self::from(image.file_id())
    }
}

impl From<&protocol::metadata::AudioFile> for FileId {
    fn from(file: &protocol::metadata::AudioFile) -> Self {
        Self::from(file.file_id())
    }
}

impl From<&protocol::metadata::VideoFile> for FileId {
    fn from(video: &protocol::metadata::VideoFile) -> Self {
        Self::from(video.file_id())
    }
}
