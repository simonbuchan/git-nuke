use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Could not find git workspace from {}", .path.display())]
    NoWorkspace { path: PathBuf },

    #[error("could not parse index file")]
    IndexParseError(#[from] IndexParseError),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub struct Workspace {
    pub root_dir: PathBuf,
    pub git_dir: PathBuf,
}

impl Workspace {
    pub fn find(dir: &Path) -> Result<Self> {
        for dir in dir.ancestors() {
            let git_dir = dir.join(".git");
            if let Ok(git_metadata) = git_dir.metadata() {
                if git_metadata.is_dir() {
                    return Ok(Self {
                        root_dir: dir.to_path_buf(),
                        git_dir,
                    });
                } else {
                    // .git is a file, so it should contain the path to the real git dir
                    if let Ok(mut git_dir_relative_path) = std::fs::read_to_string(&git_dir) {
                        git_dir_relative_path.pop(); // remove trailing newline
                        return Ok(Self {
                            root_dir: dir.to_path_buf(),
                            git_dir: dir.join(git_dir_relative_path),
                        });
                    }
                }
            }
        }
        Err(Error::NoWorkspace {
            path: dir.to_path_buf(),
        })
    }

    pub fn index(&self) -> Result<Index> {
        Ok(Index::parse(self.git_dir.join("index"))?)
    }
}

#[derive(Debug, Error)]
pub enum IndexParseError {
    #[error("io error")]
    Io(#[from] std::io::Error),
    #[error("too short: {0}")]
    TooShort(usize),
    #[error("invalid signature")]
    InvalidSignature,
    #[error("unknown version: {0}")]
    UnknownVersion(u32),
}

struct Reader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn slice(&mut self, len: usize) -> &'a [u8] {
        let slice = &self.data[self.offset..self.offset + len];
        self.offset += len;
        slice
    }

    fn array<const N: usize>(&mut self) -> [u8; N] {
        self.slice(N).try_into().unwrap()
    }

    fn vec(&mut self, len: usize) -> Vec<u8> {
        self.slice(len).to_vec()
    }

    fn skip(&mut self, len: usize) {
        self.offset += len;
    }

    fn u16(&mut self) -> u16 {
        let value = u16::from_be_bytes(self.data[self.offset..self.offset + 2].try_into().unwrap());
        self.offset += 2;
        value
    }

    fn u32(&mut self) -> u32 {
        let value = u32::from_be_bytes(self.data[self.offset..self.offset + 4].try_into().unwrap());
        self.offset += 4;
        value
    }
}

pub struct Index {
    pub version: u32,
    pub entries: Vec<IndexEntry>,
    pub extensions: Vec<IndexExtension>,
}

impl Index {
    fn parse(path: PathBuf) -> Result<Self, IndexParseError> {
        // https://git-scm.com/docs/index-format
        let data = std::fs::read(path)?;
        if data.len() < 12 {
            return Err(IndexParseError::TooShort(data.len()));
        }
        let mut reader = Reader::new(&data);
        if reader.array() != *b"DIRC" {
            return Err(IndexParseError::InvalidSignature);
        }
        let version = reader.u32();
        if !(2..=3).contains(&version) {
            return Err(IndexParseError::UnknownVersion(version));
        }
        let num_entries = reader.u32();
        let mut entries = Vec::with_capacity(num_entries as usize);
        for _ in 0..num_entries {
            let start_offset = reader.offset;
            let ctime = Timestamp {
                seconds: reader.u32(),
                nanoseconds: reader.u32(),
            };
            let mtime = Timestamp {
                seconds: reader.u32(),
                nanoseconds: reader.u32(),
            };
            let dev = reader.u32();
            let ino = reader.u32();
            let mode = reader.u32();
            let uid = reader.u32();
            let gid = reader.u32();
            let size = reader.u32();
            let sha1 = Sha1(reader.array());
            let flags = reader.u16();
            let extended_flags = if version > 2 { reader.u16() } else { 0 };
            let name_len = (flags & 0xfff) as usize;
            let name = reader.vec(name_len);
            assert_eq!(reader.data[reader.offset], 0u8);
            let name_padding = if version < 4 {
                8 - ((reader.offset - start_offset) % 8)
            } else {
                0
            };
            reader.skip(name_padding);
            entries.push(IndexEntry {
                ctime,
                mtime,
                dev,
                ino,
                mode,
                uid,
                gid,
                size,
                sha1,
                flags,
                extended_flags,
                name,
            });
        }
        let mut extensions = Vec::new();
        while reader.offset + 8 + 20 < data.len() {
            let signature = reader.array();
            let len = reader.u32() as usize;
            let data = reader.vec(len);
            extensions.push(IndexExtension { signature, data });
        }
        let _checksum = Sha1(reader.array());
        Ok(Self {
            version,
            entries,
            extensions,
        })
    }
}

#[derive(Debug)]
pub struct Sha1([u8; 20]);

impl std::fmt::Display for Sha1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

pub struct IndexEntry {
    pub ctime: Timestamp,
    pub mtime: Timestamp,
    pub dev: u32,
    pub ino: u32,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: u32,
    pub sha1: Sha1,
    pub flags: u16,
    pub extended_flags: u16,
    pub name: Vec<u8>,
}

pub struct IndexExtension {
    pub signature: [u8; 4],
    pub data: Vec<u8>,
}

pub struct Timestamp {
    pub seconds: u32,
    pub nanoseconds: u32,
}
