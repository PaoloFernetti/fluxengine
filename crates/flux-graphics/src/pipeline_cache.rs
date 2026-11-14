//! Дисковый кэш PSO (I5, ADR-0034): блоб `VkPipelineCache` + инвалидация.
//!
//! Драйверный кэш PSO валиден только для создавшего его устройства и версии
//! API. Поэтому файл начинается заголовком с подписью устройства
//! ([`DeviceSignature`]): при смене GPU, драйвера или формата данные
//! игнорируются, а не отдаются драйверу как «свой» кэш.
//!
//! Формат детерминирован (little-endian):
//! `magic[8] | version u32 | signature_hash u64 | payload_len u64 | payload`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::rhi::DeviceSignature;

/// Магия файла кэша PSO.
pub const PIPELINE_CACHE_MAGIC: [u8; 8] = *b"FLUXPSO1";

/// Версия формата заголовка.
pub const PIPELINE_CACHE_VERSION: u32 = 1;

const HEADER_LEN: usize = 8 + 4 + 8 + 8;

/// Результат чтения файла кэша.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PipelineCacheLoad {
    /// Блоб прочитан и принадлежит этому устройству.
    Loaded(Vec<u8>),
    /// Файла нет — первый запуск на устройстве.
    Missing,
    /// Файл есть, но не пригоден: кэш нужно пересобрать.
    Invalid(&'static str),
}

/// Файл кэша PSO, привязанный к подписи устройства.
#[derive(Clone, Debug)]
pub struct PipelineCacheFile {
    path: PathBuf,
    signature: DeviceSignature,
}

impl PipelineCacheFile {
    /// Создаёт описатель файла; сам файл не трогается до [`Self::load`]/[`Self::save`].
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, signature: DeviceSignature) -> Self {
        Self {
            path: path.into(),
            signature,
        }
    }

    /// Путь к файлу кэша.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Подпись устройства, которой принадлежит кэш.
    #[must_use]
    pub fn signature(&self) -> &DeviceSignature {
        &self.signature
    }

    /// Читает блоб кэша.
    ///
    /// # Errors
    ///
    /// Ошибки ввода-вывода, кроме «файла нет» (это [`PipelineCacheLoad::Missing`]).
    pub fn load(&self) -> io::Result<PipelineCacheLoad> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(PipelineCacheLoad::Missing);
            }
            Err(error) => return Err(error),
        };
        if bytes.len() < HEADER_LEN {
            return Ok(PipelineCacheLoad::Invalid("header is truncated"));
        }
        if bytes[..8] != PIPELINE_CACHE_MAGIC {
            return Ok(PipelineCacheLoad::Invalid("magic mismatch"));
        }
        let version = read_u32(&bytes[8..12]);
        if version != PIPELINE_CACHE_VERSION {
            return Ok(PipelineCacheLoad::Invalid("format version mismatch"));
        }
        let signature_hash = read_u64(&bytes[12..20]);
        if signature_hash != self.signature.hash() {
            return Ok(PipelineCacheLoad::Invalid("device signature mismatch"));
        }
        let payload_len = read_u64(&bytes[20..28]);
        let payload = &bytes[HEADER_LEN..];
        if payload.len() as u64 != payload_len {
            return Ok(PipelineCacheLoad::Invalid("payload length mismatch"));
        }
        Ok(PipelineCacheLoad::Loaded(payload.to_vec()))
    }

    /// Атомарно записывает блоб: временный файл + переименование.
    ///
    /// # Errors
    ///
    /// Ошибки ввода-вывода при создании каталога, записи или переименовании.
    pub fn save(&self, data: &[u8]) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let mut bytes = Vec::with_capacity(HEADER_LEN + data.len());
        bytes.extend_from_slice(&PIPELINE_CACHE_MAGIC);
        bytes.extend_from_slice(&PIPELINE_CACHE_VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.signature.hash().to_le_bytes());
        bytes.extend_from_slice(&(data.len() as u64).to_le_bytes());
        bytes.extend_from_slice(data);
        let tmp = self.path.with_extension("tmp");
        fs::write(&tmp, &bytes)?;
        if fs::rename(&tmp, &self.path).is_err() {
            // Windows не переименовывает поверх существующего файла.
            if self.path.exists() {
                fs::remove_file(&self.path)?;
            }
            if let Err(second) = fs::rename(&tmp, &self.path) {
                let _ = fs::remove_file(&tmp);
                return Err(second);
            }
        }
        Ok(())
    }

    /// Удаляет файл кэша (например, при смене настроек PSO).
    ///
    /// Возвращает `false`, если файла не было.
    ///
    /// # Errors
    ///
    /// Ошибки ввода-вывода, кроме «файла нет».
    pub fn invalidate(&self) -> io::Result<bool> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }
}

fn read_u32(bytes: &[u8]) -> u32 {
    let mut buffer = [0u8; 4];
    buffer.copy_from_slice(bytes);
    u32::from_le_bytes(buffer)
}

fn read_u64(bytes: &[u8]) -> u64 {
    let mut buffer = [0u8; 8];
    buffer.copy_from_slice(bytes);
    u64::from_le_bytes(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rhi::BackendKind;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn temp_path(tag: &str) -> PathBuf {
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "flux-pso-{}-{tag}-{unique}.bin",
            std::process::id()
        ))
    }

    fn signature(name: &str) -> DeviceSignature {
        DeviceSignature::new(BackendKind::Vulkan, name, 0x0040_0000)
    }

    #[test]
    fn blob_roundtrips_through_file() {
        let path = temp_path("roundtrip");
        let file = PipelineCacheFile::new(&path, signature("Test GPU"));
        assert_eq!(file.load().unwrap(), PipelineCacheLoad::Missing);
        file.save(&[1, 2, 3, 4]).unwrap();
        assert_eq!(
            file.load().unwrap(),
            PipelineCacheLoad::Loaded(vec![1, 2, 3, 4])
        );
        assert_eq!(file.signature(), &signature("Test GPU"));
        assert!(file.invalidate().unwrap());
        assert!(!file.invalidate().unwrap());
    }

    #[test]
    fn foreign_device_cache_is_invalidated() {
        let path = temp_path("foreign");
        let first = PipelineCacheFile::new(&path, signature("GPU A"));
        first.save(&[9; 32]).unwrap();
        let second = PipelineCacheFile::new(&path, signature("GPU B"));
        assert_eq!(
            second.load().unwrap(),
            PipelineCacheLoad::Invalid("device signature mismatch")
        );
        // Валидный владелец по-прежнему читает свой блоб.
        assert!(matches!(
            first.load().unwrap(),
            PipelineCacheLoad::Loaded(ref data) if data == &[9; 32]
        ));
    }

    #[test]
    fn corrupted_and_truncated_files_are_reported() {
        let path = temp_path("corrupted");
        let file = PipelineCacheFile::new(&path, signature("GPU"));
        fs::write(&path, b"FLUX").unwrap();
        assert_eq!(
            file.load().unwrap(),
            PipelineCacheLoad::Invalid("header is truncated")
        );
        fs::write(&path, b"NOTACACHE-magic-but-long-enough-payload").unwrap();
        assert_eq!(
            file.load().unwrap(),
            PipelineCacheLoad::Invalid("magic mismatch")
        );
        file.save(&[7; 8]).unwrap();
        let mut bytes = fs::read(&path).unwrap();
        bytes.truncate(bytes.len() - 1);
        fs::write(&path, bytes).unwrap();
        assert_eq!(
            file.load().unwrap(),
            PipelineCacheLoad::Invalid("payload length mismatch")
        );
        let _ = fs::remove_file(&path);
    }
}
