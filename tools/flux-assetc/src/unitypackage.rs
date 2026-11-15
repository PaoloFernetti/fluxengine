//! Распаковка Unity `.unitypackage` (ADR-0043).
//!
//! Пакет — это gzip-сжатый tar, где каждый ассет лежит в каталоге GUID:
//! `pathname` (исходный путь), `asset` (содержимое), `asset.meta` (Unity-meta),
//! опционально `preview.png`. Распаковщик:
//!
//! 1. извлекает архив во временный staging-каталог, принимая только записи
//!    `GUID/файл`;
//! 2. читает `pathname`, нормализует `\` → `/` и отвергает абсолютные пути,
//!    `..`, `.`, префиксы дисков и пустые компоненты;
//! 3. копирует `asset` в `output/<pathname>`, а `asset.meta` — рядом
//!    (`<pathname>.meta`); пути с `/` на конце создают каталог;
//! 4. сортирует GUID, поэтому результат детерминирован.
//!
//! Ошибки типизированы, паники на недоверенном вводе нет.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use flate2::read::GzDecoder;
use tar::Archive;

/// Параметры распаковки.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractOptions {
    /// Каталог, в который восстанавливается дерево `Assets/...`.
    pub output: PathBuf,
    /// Писать `asset.meta` рядом с ассетом (`<path>.meta`).
    pub keep_meta: bool,
    /// Перезаписывать существующие файлы (иначе — пропуск).
    pub overwrite: bool,
    /// Только показать план: staging читается, `output` не меняется.
    pub dry_run: bool,
}

impl ExtractOptions {
    /// Параметры по умолчанию: meta сохраняется, существующие файлы не
    /// перезаписываются, `dry_run` выключен.
    #[must_use]
    pub fn new(output: impl Into<PathBuf>) -> Self {
        Self {
            output: output.into(),
            keep_meta: true,
            overwrite: false,
            dry_run: false,
        }
    }
}

/// Итог распаковки.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtractReport {
    /// Сколько ассетов записано (или было бы записано при `dry_run`).
    pub assets: usize,
    /// Сколько папок создано.
    pub folders: usize,
    /// Сколько записей пропущено (нет `asset`, файл уже существует).
    pub skipped: usize,
    /// Суммарный размер записанных ассетов, байт.
    pub bytes: u64,
    /// Относительные пути в порядке обработки.
    pub paths: Vec<PathBuf>,
}

/// Ошибка распаковки.
#[derive(Debug)]
pub enum UnityPackageError {
    /// Ошибка ввода-вывода, gzip или tar.
    Io(io::Error),
    /// У GUID-каталога нет `pathname`.
    MissingPathname(String),
    /// `pathname` пустой или выходит за пределы `--out`.
    UnsafePath {
        /// GUID записи.
        guid: String,
        /// Исходный `pathname`.
        path: String,
    },
}

impl fmt::Display for UnityPackageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "unitypackage: {error}"),
            Self::MissingPathname(guid) => {
                write!(formatter, "unitypackage: у GUID {guid} нет pathname")
            }
            Self::UnsafePath { guid, path } => write!(
                formatter,
                "unitypackage: небезопасный pathname {path:?} у GUID {guid}"
            ),
        }
    }
}

impl std::error::Error for UnityPackageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for UnityPackageError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Распаковывает `.unitypackage` в `options.output`.
///
/// # Errors
///
/// [`UnityPackageError`]: битый gzip/tar, отсутствующий `pathname`,
/// небезопасный путь или ошибка записи.
pub fn extract(
    archive: &Path,
    options: &ExtractOptions,
) -> Result<ExtractReport, UnityPackageError> {
    let staging = staging_dir();
    fs::create_dir_all(&staging)?;
    let result = extract_inner(archive, options, &staging);
    let _ = fs::remove_dir_all(&staging);
    result
}

fn extract_inner(
    archive: &Path,
    options: &ExtractOptions,
    staging: &Path,
) -> Result<ExtractReport, UnityPackageError> {
    let file = fs::File::open(archive)?;
    let decoder = GzDecoder::new(file);
    let mut tar = Archive::new(decoder);
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let mut components = path.components();
        let (Some(Component::Normal(guid)), Some(Component::Normal(member))) =
            (components.next(), components.next())
        else {
            continue;
        };
        if components.next().is_some() {
            continue;
        }
        let (Some(guid), Some(member)) = (guid.to_str(), member.to_str()) else {
            continue;
        };
        if !is_safe_component(guid) || !is_safe_component(member) {
            continue;
        }
        let directory = staging.join(guid);
        fs::create_dir_all(&directory)?;
        entry.unpack(directory.join(member))?;
    }
    collect(options, staging)
}

fn collect(options: &ExtractOptions, staging: &Path) -> Result<ExtractReport, UnityPackageError> {
    let mut directories: Vec<PathBuf> = fs::read_dir(staging)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    directories.sort();

    let mut report = ExtractReport::default();
    for directory in directories {
        let guid = directory
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("?")
            .to_owned();
        let pathname = match fs::read_to_string(directory.join("pathname")) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(UnityPackageError::MissingPathname(guid));
            }
            Err(error) => return Err(error.into()),
        };
        let raw = pathname.trim();
        if raw.is_empty() {
            return Err(UnityPackageError::UnsafePath {
                guid,
                path: pathname,
            });
        }
        let relative = sanitize_relative(raw).ok_or_else(|| UnityPackageError::UnsafePath {
            guid: guid.clone(),
            path: raw.to_owned(),
        })?;
        let meta = directory.join("asset.meta");
        let meta_text = fs::read_to_string(&meta).unwrap_or_default();
        let is_folder =
            raw.ends_with('/') || raw.ends_with('\\') || meta_text.contains("folderAsset: yes");
        let asset = directory.join("asset");

        if is_folder {
            if !options.dry_run {
                let target = options.output.join(&relative);
                fs::create_dir_all(&target)?;
                if options.keep_meta && meta.is_file() {
                    fs::copy(&meta, with_meta_suffix(&target))?;
                }
            }
            report.folders += 1;
            report.paths.push(relative);
            continue;
        }

        if !asset.is_file() {
            report.skipped += 1;
            continue;
        }

        let target = options.output.join(&relative);
        if target.exists() && !options.overwrite {
            report.skipped += 1;
            continue;
        }

        let bytes = fs::metadata(&asset)?.len();
        if !options.dry_run {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&asset, &target)?;
            if options.keep_meta && meta.is_file() {
                fs::copy(&meta, with_meta_suffix(&target))?;
            }
        }

        report.assets += 1;
        report.bytes = report.bytes.saturating_add(bytes);
        report.paths.push(relative);
    }
    Ok(report)
}

fn with_meta_suffix(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".meta");
    PathBuf::from(name)
}

fn is_safe_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.contains('/')
        && !value.contains('\\')
        && !value.contains(':')
        && !value.contains('\0')
}

fn sanitize_relative(raw: &str) -> Option<PathBuf> {
    let normalized = raw.replace('\\', "/");
    let trimmed = normalized.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let mut relative = PathBuf::new();
    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(part) => {
                let text = part.to_str()?;
                if text.is_empty() || text == "." || text.contains(':') {
                    return None;
                }
                relative.push(text);
            }
            _ => return None,
        }
    }
    if relative.as_os_str().is_empty() {
        None
    } else {
        Some(relative)
    }
}

fn staging_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("flux-unitypackage-{}-{nanos}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;
    use tar::{Builder, Header};

    fn package(path: &Path, entries: &[(&str, &[u8])]) {
        let file = fs::File::create(path).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);
        for (name, data) in entries {
            append(&mut builder, name, data);
        }
        builder.into_inner().unwrap().finish().unwrap();
    }

    fn append<W: Write>(builder: &mut Builder<W>, name: &str, data: &[u8]) {
        let mut header = Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, name, data).unwrap();
    }

    fn sample(dir: &Path) -> PathBuf {
        let archive = dir.join("sample.unitypackage");
        package(
            &archive,
            &[
                ("aaaa/pathname", b"Assets/Textures/rock.png"),
                ("aaaa/asset", b"PNGDATA"),
                ("aaaa/asset.meta", b"guid: aaaa\n"),
                ("bbbb/pathname", b"Assets/Textures/Folder"),
                ("bbbb/asset.meta", b"folderAsset: yes\nguid: bbbb\n"),
            ],
        );
        archive
    }

    #[test]
    fn extracts_assets_meta_and_folders() {
        let dir = tempfile::tempdir().unwrap();
        let archive = sample(dir.path());
        let output = dir.path().join("out");

        let report = extract(&archive, &ExtractOptions::new(&output)).unwrap();
        assert_eq!(report.assets, 1);
        assert_eq!(report.folders, 1);
        assert_eq!(report.skipped, 0);
        assert_eq!(report.bytes, 7);
        assert_eq!(report.paths.len(), 2);
        assert_eq!(
            fs::read(output.join("Assets/Textures/rock.png")).unwrap(),
            b"PNGDATA"
        );
        assert_eq!(
            fs::read(output.join("Assets/Textures/rock.png.meta")).unwrap(),
            b"guid: aaaa\n"
        );
        assert!(output.join("Assets/Textures/Folder").is_dir());
        assert_eq!(
            fs::read(output.join("Assets/Textures/Folder.meta")).unwrap(),
            b"folderAsset: yes\nguid: bbbb\n"
        );
    }

    #[test]
    fn unsafe_pathname_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("evil.unitypackage");
        package(
            &archive,
            &[("cccc/pathname", b"../escape.txt"), ("cccc/asset", b"nope")],
        );
        let output = dir.path().join("out");
        let error = extract(&archive, &ExtractOptions::new(&output)).unwrap_err();
        assert!(matches!(error, UnityPackageError::UnsafePath { .. }));
        assert!(!dir.path().join("escape.txt").exists());
    }

    #[test]
    fn dry_run_does_not_write() {
        let dir = tempfile::tempdir().unwrap();
        let archive = sample(dir.path());
        let output = dir.path().join("out");
        let mut options = ExtractOptions::new(&output);
        options.dry_run = true;

        let report = extract(&archive, &options).unwrap();
        assert_eq!(report.assets, 1);
        assert!(!output.exists());
    }

    #[test]
    fn existing_files_are_skipped_without_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let archive = sample(dir.path());
        let output = dir.path().join("out");
        let target = output.join("Assets/Textures/rock.png");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, b"OLD").unwrap();

        let report = extract(&archive, &ExtractOptions::new(&output)).unwrap();
        assert_eq!(report.assets, 0);
        assert_eq!(report.skipped, 1);
        assert_eq!(fs::read(&target).unwrap(), b"OLD");

        let mut overwrite = ExtractOptions::new(&output);
        overwrite.overwrite = true;
        let report = extract(&archive, &overwrite).unwrap();
        assert_eq!(report.assets, 1);
        assert_eq!(fs::read(&target).unwrap(), b"PNGDATA");
    }

    #[test]
    fn missing_pathname_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("broken.unitypackage");
        package(&archive, &[("dddd/asset", b"x")]);
        let error = extract(&archive, &ExtractOptions::new(dir.path().join("out"))).unwrap_err();
        assert!(matches!(error, UnityPackageError::MissingPathname(guid) if guid == "dddd"));
    }
}
