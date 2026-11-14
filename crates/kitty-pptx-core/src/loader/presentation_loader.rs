use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use ooxml_common::resource::{ResourceUsage, ResourceViolation};
use crate::error::ParseError;
use super::asset_store::{AssetStore, DefaultAssetStore};

pub enum Source {
    File(PathBuf),
    Stdin,
    Bytes(Vec<u8>),
}

pub trait PresentationLoader: Send + Sync {
    fn load(
        &self,
        src: Source,
    ) -> Result<(pptx_model::Presentation, Arc<dyn AssetStore>), ParseError>;
}

pub struct DefaultPresentationLoader;

impl PresentationLoader for DefaultPresentationLoader {
    fn load(
        &self,
        src: Source,
    ) -> Result<(pptx_model::Presentation, Arc<dyn AssetStore>), ParseError> {
        let (bytes, temp_file): (Arc<[u8]>, Option<tempfile::NamedTempFile>) = match src {
            Source::File(path) => {
                let data = std::fs::read(&path)
                    .map_err(|e| ParseError::ArchiveError(e.to_string()))?;
                (Arc::from(data.into_boxed_slice()), None)
            }
            Source::Bytes(vec) => (Arc::from(vec.into_boxed_slice()), None),
            Source::Stdin => {
                // Spool up to 32 MB in RAM; spill to tempfile up to 256 MB limit
                let mut buffer = Vec::new();
                let stdin = std::io::stdin();
                let mut handle = stdin.lock();

                let mut chunk = [0u8; 64 * 1024];
                let mut total_read = 0;
                let ram_limit = 32 * 1024 * 1024;
                let hard_limit = 256 * 1024 * 1024;

                let mut spilled_temp: Option<tempfile::NamedTempFile> = None;

                loop {
                    let n = handle
                        .read(&mut chunk)
                        .map_err(|e| ParseError::ArchiveError(e.to_string()))?;
                    if n == 0 {
                        break;
                    }
                    total_read += n;
                    if total_read > hard_limit {
                        return Err(ParseError::ResourceLimit(Box::new(ResourceViolation {
                            stage: "container",
                            resource: "stdin-stream",
                            metric: "bytes",
                            part: None,
                            limit: hard_limit as u64,
                            observed: total_read as u64,
                            configurable: false,
                            usage: ResourceUsage::default(),
                        })));
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

        // Parse using vendored native parser. A governed ceiling crossing keeps
        // its typed violation so callers can tell a quota from corruption.
        let presentation = pptx_parser::parse_pptx_model_native_checked(&bytes).map_err(
            |error| match error {
                pptx_parser::PptxError::Resource(violation) => ParseError::ResourceLimit(violation),
                pptx_parser::PptxError::Format(message) => ParseError::ArchiveError(message),
            },
        )?;

        let asset_store: Arc<dyn AssetStore> = Arc::new(DefaultAssetStore::new(bytes, temp_file));

        Ok((presentation, asset_store))
    }
}
