use std::io::{Read, Seek, Cursor};
use std::path::PathBuf;
use std::sync::Arc;
use crate::error::{ParseError, AssetError};

pub enum Source {
    File(PathBuf),
    Stdin,
    Bytes(Vec<u8>),
}

pub enum SizeHint {
    Native,
    AtMost { width: u32, height: u32 },
}

pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Arc<[u8]>, // Straight RGBA8
}

pub struct AssetStoreStats {
    pub cached_entries: usize,
    pub cached_bytes: usize,
}

pub trait AssetStore: Send + Sync {
    fn get(&self, rel_id: &str, hint: SizeHint) -> Result<DecodedImage, AssetError>;
    fn stats(&self) -> AssetStoreStats;
}

pub trait PresentationLoader: Send + Sync {
    fn load(&self, src: Source) -> Result<(pptx_model::Presentation, Arc<dyn AssetStore>), ParseError>;
}

/// In-memory & spooled archive asset store implementation
pub struct DefaultAssetStore {
    archive_bytes: Arc<[u8]>,
    _spooled_temp_file: Option<tempfile::NamedTempFile>,
}

impl DefaultAssetStore {
    pub fn new(bytes: Arc<[u8]>, temp_file: Option<tempfile::NamedTempFile>) -> Self {
        Self {
            archive_bytes: bytes,
            _spooled_temp_file: temp_file,
        }
    }
}

impl AssetStore for DefaultAssetStore {
    fn get(&self, rel_id: &str, _hint: SizeHint) -> Result<DecodedImage, AssetError> {
        let cursor = Cursor::new(&self.archive_bytes[..]);
        let mut zip = zip::ZipArchive::new(cursor)
            .map_err(|e| AssetError::DecodeFailed(e.to_string()))?;
        
        let mut entry = zip.by_name(rel_id)
            .map_err(|_| AssetError::NotFound(rel_id.to_string()))?;
        
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data)
            .map_err(|e| AssetError::DecodeFailed(e.to_string()))?;

        // Decode via image crate
        let img = image::load_from_memory(&data)
            .map_err(|e| AssetError::DecodeFailed(e.to_string()))?;
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();

        Ok(DecodedImage {
            width,
            height,
            pixels: Arc::from(rgba.into_raw().into_boxed_slice()),
        })
    }

    fn stats(&self) -> AssetStoreStats {
        AssetStoreStats {
            cached_entries: 0,
            cached_bytes: 0,
        }
    }
}

pub struct DefaultPresentationLoader;

impl PresentationLoader for DefaultPresentationLoader {
    fn load(&self, src: Source) -> Result<(pptx_model::Presentation, Arc<dyn AssetStore>), ParseError> {
        let (bytes, temp_file): (Arc<[u8]>, Option<tempfile::NamedTempFile>) = match src {
            Source::File(path) => {
                let data = std::fs::read(&path)
                    .map_err(|e| ParseError::ArchiveError(e.to_string()))?;
                (Arc::from(data.into_boxed_slice()), None)
            }
            Source::Bytes(vec) => {
                (Arc::from(vec.into_boxed_slice()), None)
            }
            Source::Stdin => {
                // Spool up to 32 MB in RAM; spill to tempfile up to 256 MB limit
                let mut buffer = Vec::new();
                let stdin = std::io::stdin();
                let mut handle = stdin.lock();
                
                // Read first 32 MB
                let mut chunk = [0u8; 64 * 1024];
                let mut total_read = 0;
                let ram_limit = 32 * 1024 * 1024;
                let hard_limit = 256 * 1024 * 1024;

                let mut spilled_temp: Option<tempfile::NamedTempFile> = None;

                loop {
                    let n = handle.read(&mut chunk).map_err(|e| ParseError::ArchiveError(e.to_string()))?;
                    if n == 0 { break; }
                    total_read += n;
                    if total_read > hard_limit {
                        return Err(ParseError::ResourceLimitHit("stdin stream exceeded 256 MB"));
                    }

                    if spilled_temp.is_none() && total_read <= ram_limit {
                        buffer.extend_from_slice(&chunk[..n]);
                    } else {
                        // Spill to disk
                        if spilled_temp.is_none() {
                            let mut temp = tempfile::NamedTempFile::new()
                                .map_err(|e| ParseError::ArchiveError(e.to_string()))?;
                            use std::io::Write;
                            temp.write_all(&buffer)
                                .map_err(|e| ParseError::ArchiveError(e.to_string()))?;
                            buffer.clear();
                            buffer.shrink_to_fit();
                            spilled_temp = Some(temp);
                        }
                        if let Some(ref mut temp) = spilled_temp {
                            use std::io::Write;
                            temp.write_all(&chunk[..n])
                                .map_err(|e| ParseError::ArchiveError(e.to_string()))?;
                        }
                    }
                }

                if let Some(mut temp) = spilled_temp {
                    use std::io::SeekFrom;
                    temp.seek(SeekFrom::Start(0))
                        .map_err(|e| ParseError::ArchiveError(e.to_string()))?;
                    let mut data = Vec::with_capacity(total_read);
                    temp.read_to_end(&mut data)
                        .map_err(|e| ParseError::ArchiveError(e.to_string()))?;
                    (Arc::from(data.into_boxed_slice()), Some(temp))
                } else {
                    (Arc::from(buffer.into_boxed_slice()), None)
                }
            }
        };

        // Parse using vendored native parser
        let presentation = pptx_parser::parse_pptx_model_native(&bytes[..])
            .map_err(|e| ParseError::ArchiveError(format!("{:?}", e)))?;

        let asset_store: Arc<dyn AssetStore> = Arc::new(DefaultAssetStore::new(bytes, temp_file));

        Ok((presentation, asset_store))
    }
}
