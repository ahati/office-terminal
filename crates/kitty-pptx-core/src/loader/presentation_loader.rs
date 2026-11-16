use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use ooxml_common::resource::{ResourceUsage, ResourceViolation};
use crate::contracts::{DeckSource, PageSource, SourceFormat};
use crate::error::ParseError;
use super::asset_store::{AssetStore, DefaultAssetStore};

pub enum Source {
    File(PathBuf),
    Stdin,
    Bytes(Vec<u8>),
}

pub type PdfLoaderFn = fn(Arc<[u8]>) -> Result<Arc<dyn PageSource>, ParseError>;
static PDF_LOADER: OnceLock<PdfLoaderFn> = OnceLock::new();

/// Registers a PDF engine loader hook.
pub fn register_pdf_loader(loader: PdfLoaderFn) {
    let _ = PDF_LOADER.set(loader);
}

pub trait PresentationLoader: Send + Sync {
    fn load(
        &self,
        src: Source,
    ) -> Result<(pptx_model::Presentation, Arc<dyn AssetStore>), ParseError>;

    fn load_deck(&self, src: Source) -> Result<DeckSource, ParseError> {
        let (bytes, temp_file) = read_source(src)?;
        match sniff_container_format(&bytes)? {
            SourceFormat::Pptx => {
                let presentation = pptx_parser::parse_pptx_model_native_checked(&bytes).map_err(
                    |error| match error {
                        pptx_parser::PptxError::Resource(violation) => ParseError::ResourceLimit(violation),
                        pptx_parser::PptxError::Format(message) => ParseError::ArchiveError(message),
                    },
                )?;
                let asset_store: Arc<dyn AssetStore> = Arc::new(DefaultAssetStore::new(bytes, temp_file));
                Ok(DeckSource::Pptx(presentation, asset_store))
            }
            SourceFormat::Pdf => {
                if let Some(loader) = PDF_LOADER.get() {
                    let page_source = loader(bytes)?;
                    Ok(DeckSource::Pdf(page_source))
                } else {
                    Err(ParseError::ArchiveError(
                        "PDF engine is not registered or linked".to_string(),
                    ))
                }
            }
        }
    }
}

pub struct DefaultPresentationLoader;

impl PresentationLoader for DefaultPresentationLoader {
    fn load(
        &self,
        src: Source,
    ) -> Result<(pptx_model::Presentation, Arc<dyn AssetStore>), ParseError> {
        let (bytes, temp_file) = read_source(src)?;
        sniff_container(&bytes)?;

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

fn read_source(src: Source) -> Result<(Arc<[u8]>, Option<tempfile::NamedTempFile>), ParseError> {
    match src {
        Source::File(path) => {
            let data = std::fs::read(&path)
                .map_err(|e| ParseError::ArchiveError(e.to_string()))?;
            Ok((Arc::from(data.into_boxed_slice()), None))
        }
        Source::Bytes(vec) => Ok((Arc::from(vec.into_boxed_slice()), None)),
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
                Ok((Arc::from(data.into_boxed_slice()), Some(temp)))
            } else {
                Ok((Arc::from(buffer.into_boxed_slice()), None))
            }
        }
    }
}

/// CFB/OLE2 magic (`D0 CF 11 E0 A1 B1 1A E1`).
const CFB_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

fn utf16le_contains(haystack: &[u8], needle: &str) -> bool {
    let pattern: Vec<u8> = needle
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    !pattern.is_empty()
        && haystack
            .windows(pattern.len())
            .any(|window| window == pattern.as_slice())
}

/// Sniffs the document format from magic bytes.
pub fn sniff_container_format(bytes: &[u8]) -> Result<SourceFormat, ParseError> {
    let check_len = bytes.len().min(1024);
    if bytes[..check_len].windows(5).any(|w| w == b"%PDF-") {
        return Ok(SourceFormat::Pdf);
    }
    if bytes.starts_with(&CFB_MAGIC) {
        if utf16le_contains(bytes, "EncryptedPackage") {
            return Err(ParseError::PasswordProtected);
        }
        if utf16le_contains(bytes, "PowerPoint Document") {
            return Err(ParseError::ArchiveError(
                "legacy binary PowerPoint (.ppt) is not an OOXML package; convert the deck to .pptx"
                    .to_string(),
            ));
        }
        return Err(ParseError::ArchiveError(
            "unsupported CFB/OLE2 container: not an OOXML package".to_string(),
        ));
    }
    if bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
    {
        return Ok(SourceFormat::Pptx);
    }
    Err(ParseError::ArchiveError(
        "not an OOXML ZIP package (expected .pptx/.pptm OPC container) or PDF document".to_string(),
    ))
}

/// Reject non-OOXML containers with a typed error instead of letting the ZIP
/// reader report "corrupted archive".
pub fn sniff_container(bytes: &[u8]) -> Result<(), ParseError> {
    match sniff_container_format(bytes)? {
        SourceFormat::Pptx => Ok(()),
        SourceFormat::Pdf => Err(ParseError::ArchiveError(
            "PDF document passed to OOXML ZIP parser".to_string(),
        )),
    }
}
