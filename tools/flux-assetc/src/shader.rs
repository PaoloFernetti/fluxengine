//! Офлайн-компиляция HLSL → SPIR-V через DXC (ADR-0014, ADR-0035).
//!
//! DXC не входит в репозиторий: путь ищется в `DXC_PATH`, затем в `PATH`.
//! Отсутствие компилятора — типизированная ошибка, а не паника: CI без DXC
//! остаётся зелёным, а сборка шейдеров запускается отдельным шагом.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Параметры компиляции.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DxcOptions {
    /// Entry point (`vs_main`, `fs_main`, `cs_main`).
    pub entry: String,
    /// Профиль стадии (`vs_6_0`, `ps_6_0`, `cs_6_0`).
    pub stage: String,
    /// Определения препроцессора.
    pub defines: Vec<(String, String)>,
    /// Каталоги include.
    pub includes: Vec<PathBuf>,
}

/// Ошибки компиляции шейдеров.
#[derive(Debug)]
pub enum ShaderCompileError {
    /// DXC не найден.
    DxcNotFound,
    /// Ошибка ввода-вывода/запуска процесса.
    Io(String),
    /// DXC завершился с ошибкой.
    Failed {
        /// Код возврата.
        code: Option<i32>,
        /// stderr компилятора.
        stderr: String,
    },
    /// Некорректные параметры.
    InvalidOptions(String),
}

impl fmt::Display for ShaderCompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DxcNotFound => {
                formatter.write_str("DXC не найден: задайте DXC_PATH или добавьте dxc в PATH")
            }
            Self::Io(error) => write!(formatter, "ошибка запуска DXC: {error}"),
            Self::Failed { code, stderr } => {
                write!(formatter, "DXC завершился с кодом {code:?}: {stderr}")
            }
            Self::InvalidOptions(reason) => write!(formatter, "некорректные параметры: {reason}"),
        }
    }
}

impl std::error::Error for ShaderCompileError {}

/// Кандидаты вендорного DXC относительно корня проекта.
///
/// Бинарники не коммитятся (см. `tools/dxc/README.md`): их кладёт CI или
/// разработчик из официального NuGet/SDK, а `DXC_PATH` может переопределить.
#[must_use]
pub fn vendored_candidates(root: &Path) -> Vec<PathBuf> {
    let exe = if cfg!(windows) { "dxc.exe" } else { "dxc" };
    vec![
        root.join("tools").join("dxc").join("bin").join(exe),
        root.join("tools").join("dxc").join(exe),
    ]
}

/// Ищет DXC: `DXC_PATH`, рядом с бинарником, вендорный каталог, затем `PATH`.
#[must_use]
pub fn find_dxc() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("DXC_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let exe = if cfg!(windows) { "dxc.exe" } else { "dxc" };

    if let Ok(current) = std::env::current_exe() {
        if let Some(directory) = current.parent() {
            let candidates = [directory.join(exe), directory.join("dxc").join(exe)];
            if let Some(found) = candidates.into_iter().find(|path| path.is_file()) {
                return Some(found);
            }
        }
    }

    if let Ok(root) = std::env::current_dir() {
        if let Some(found) = vendored_candidates(&root)
            .into_iter()
            .find(|path| path.is_file())
        {
            return Some(found);
        }
    }

    let path = std::env::var_os("PATH")?;
    find_in_paths(std::env::split_paths(&path), exe)
}

/// Ищет исполняемый файл в перечне каталогов.
#[must_use]
pub fn find_in_paths(paths: impl IntoIterator<Item = PathBuf>, exe: &str) -> Option<PathBuf> {
    paths
        .into_iter()
        .map(|directory| directory.join(exe))
        .find(|candidate| candidate.is_file())
}

/// Компилирует HLSL в SPIR-V, используя найденный DXC.
///
/// # Errors
///
/// [`ShaderCompileError`] — DXC не найден, параметры некорректны или
/// компилятор вернул ошибку.
pub fn compile(hlsl: &Path, output: &Path, options: &DxcOptions) -> Result<(), ShaderCompileError> {
    let dxc = find_dxc().ok_or(ShaderCompileError::DxcNotFound)?;
    compile_with(&dxc, hlsl, output, options)
}

/// Компилирует HLSL конкретным исполняемым файлом DXC.
///
/// # Errors
///
/// [`ShaderCompileError`] — параметры некорректны, файла нет или компилятор
/// вернул ошибку.
pub fn compile_with(
    dxc: &Path,
    hlsl: &Path,
    output: &Path,
    options: &DxcOptions,
) -> Result<(), ShaderCompileError> {
    if options.entry.is_empty() || options.stage.is_empty() {
        return Err(ShaderCompileError::InvalidOptions(
            "нужны entry point и stage".to_owned(),
        ));
    }
    if !hlsl.is_file() {
        return Err(ShaderCompileError::Io(format!(
            "нет исходника {}",
            hlsl.display()
        )));
    }
    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|error| ShaderCompileError::Io(error.to_string()))?;
        }
    }

    // На Windows .cmd/.bat нельзя запустить напрямую через CreateProcess:
    // оборачиваем в `cmd /C`, чтобы поддерживать скрипты-обёртки DXC.
    let mut command = if cfg!(windows)
        && matches!(
            dxc.extension().and_then(|extension| extension.to_str()),
            Some("cmd" | "bat")
        ) {
        let mut command = Command::new("cmd");
        command.arg("/C").arg(dxc);
        command
    } else {
        Command::new(dxc)
    };
    command
        .arg("-T")
        .arg(&options.stage)
        .arg("-E")
        .arg(&options.entry)
        .arg("-spirv")
        .arg("-Fo")
        .arg(output);
    for (name, value) in &options.defines {
        command.arg("-D").arg(format!("{name}={value}"));
    }
    for include in &options.includes {
        command.arg("-I").arg(include);
    }
    command.arg(hlsl);

    let result = command
        .output()
        .map_err(|error| ShaderCompileError::Io(error.to_string()))?;
    if result.status.success() {
        Ok(())
    } else {
        Err(ShaderCompileError::Failed {
            code: result.status.code(),
            stderr: String::from_utf8_lossy(&result.stderr).into_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("flux-assetc-shader-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[cfg(windows)]
    fn fake_dxc(directory: &Path, failing: bool) -> PathBuf {
        let path = directory.join("dxc.cmd");
        let body = if failing {
            "@echo off\r\necho boom 1>&2\r\nexit /b 3\r\n"
        } else {
            "@echo off\r\n:loop\r\nif \"%~1\"==\"-Fo\" (\r\n  <nul set /p \"=SPRV\" > \"%~2\"\r\n  exit /b 0\r\n)\r\nshift\r\nif not \"%~1\"==\"\" goto loop\r\nexit /b 1\r\n"
        };
        std::fs::write(&path, body).expect("fake dxc");
        path
    }

    #[cfg(unix)]
    fn fake_dxc(directory: &Path, failing: bool) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = directory.join("dxc");
        let body = if failing {
            "#!/bin/sh\necho boom 1>&2\nexit 3\n".to_owned()
        } else {
            "#!/bin/sh\nwhile [ $# -gt 0 ]; do\n  if [ \"$1\" = \"-Fo\" ]; then\n    printf SPRV > \"$2\"\n    exit 0\n  fi\n  shift\ndone\nexit 1\n".to_owned()
        };
        std::fs::write(&path, body).expect("fake dxc");
        let mut permissions = std::fs::metadata(&path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("chmod");
        path
    }

    fn options() -> DxcOptions {
        DxcOptions {
            entry: "vs_main".to_owned(),
            stage: "vs_6_0".to_owned(),
            defines: vec![("FLUX".to_owned(), "1".to_owned())],
            includes: Vec::new(),
        }
    }

    #[test]
    fn find_in_paths_returns_existing_file() {
        let dir = temp_dir();
        let exe = dir.join("dxc-test");
        std::fs::write(&exe, b"stub").expect("stub");
        let found = find_in_paths(vec![dir.join("missing"), dir.clone()], "dxc-test");
        assert_eq!(found, Some(exe));
    }

    #[test]
    fn vendored_candidates_follow_layout() {
        let root = temp_dir();
        let candidates = vendored_candidates(&root);
        assert_eq!(candidates.len(), 2);
        assert!(candidates[0].starts_with(root.join("tools").join("dxc")));
        assert_eq!(
            candidates[0].file_name().and_then(|name| name.to_str()),
            Some(if cfg!(windows) { "dxc.exe" } else { "dxc" })
        );
    }

    #[test]
    fn missing_binary_is_a_typed_error() {
        let dir = temp_dir();
        let hlsl = dir.join("test.hlsl");
        std::fs::write(&hlsl, b"float4 main() : SV_Position { return 0; }").expect("hlsl");
        let result = compile_with(
            &dir.join("dxc-missing"),
            &hlsl,
            &dir.join("out.spv"),
            &options(),
        );
        assert!(matches!(result, Err(ShaderCompileError::Io(_))));
    }

    #[test]
    fn empty_options_are_rejected() {
        let dir = temp_dir();
        let hlsl = dir.join("test.hlsl");
        std::fs::write(&hlsl, b"//").expect("hlsl");
        let result = compile_with(
            &dir.join("dxc"),
            &hlsl,
            &dir.join("out.spv"),
            &DxcOptions::default(),
        );
        assert!(matches!(result, Err(ShaderCompileError::InvalidOptions(_))));
    }

    #[test]
    fn fake_dxc_produces_output() {
        let dir = temp_dir();
        let dxc = fake_dxc(&dir, false);
        let hlsl = dir.join("test.hlsl");
        std::fs::write(&hlsl, b"float4 main() : SV_Position { return 0; }").expect("hlsl");
        let output = dir.join("out/test.spv");
        compile_with(&dxc, &hlsl, &output, &options()).expect("compile");
        let bytes = std::fs::read(&output).expect("spirv");
        assert_eq!(bytes, b"SPRV");
    }

    #[test]
    fn compiler_failure_reports_stderr() {
        let dir = temp_dir();
        let dxc = fake_dxc(&dir, true);
        let hlsl = dir.join("test.hlsl");
        std::fs::write(&hlsl, b"//").expect("hlsl");
        let result = compile_with(&dxc, &hlsl, &dir.join("out.spv"), &options());
        match result {
            Err(ShaderCompileError::Failed { code, stderr }) => {
                assert_eq!(code, Some(3));
                assert!(stderr.contains("boom"));
            }
            other => panic!("ожидалась ошибка компилятора, получено {other:?}"),
        }
    }
}
