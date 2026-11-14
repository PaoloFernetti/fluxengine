//! Компилятор ассетов: KTX2, меши, Opus, .fpf, шейдеры.
//!
//! Этап 4 (I6b): `shader` — офлайн-компиляция HLSL → SPIR-V через DXC.
//! Остальные подкоманды появятся на этапе 5.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod shader;

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("flux-assetc: {message}");
            ExitCode::from(2)
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("shader") => shader_command(&args[1..]),
        Some(other) => Err(format!("неизвестная команда {other:?}")),
        None => Err(usage()),
    }
}

fn usage() -> String {
    "использование: flux-assetc shader <input.hlsl> --out <file.spv> \
     --entry <name> --stage <profile> [--define NAME=VALUE]... [--include DIR]... | \
     flux-assetc shader --check"
        .to_owned()
}

fn shader_command(args: &[String]) -> Result<(), String> {
    if args.first().map(String::as_str) == Some("--check") {
        return match shader::find_dxc() {
            Some(path) => {
                println!("DXC: {}", path.display());
                Ok(())
            }
            None => Err(shader::ShaderCompileError::DxcNotFound.to_string()),
        };
    }

    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut options = shader::DxcOptions::default();
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        let value = |index: usize| -> Result<&String, String> {
            args.get(index + 1)
                .ok_or_else(|| format!("у флага {argument} нет значения"))
        };
        match argument.as_str() {
            "--out" => {
                output = Some(PathBuf::from(value(index)?));
                index += 2;
            }
            "--entry" => {
                options.entry = value(index)?.clone();
                index += 2;
            }
            "--stage" => {
                options.stage = value(index)?.clone();
                index += 2;
            }
            "--define" => {
                let definition = value(index)?;
                let (name, value) = definition.split_once('=').ok_or_else(|| {
                    format!("--define ожидает NAME=VALUE, получено {definition:?}")
                })?;
                options.defines.push((name.to_owned(), value.to_owned()));
                index += 2;
            }
            "--include" => {
                options.includes.push(PathBuf::from(value(index)?));
                index += 2;
            }
            other if other.starts_with("--") => {
                return Err(format!("неизвестный флаг {other}"));
            }
            other => {
                if input.is_some() {
                    return Err(format!("лишний аргумент {other:?}"));
                }
                input = Some(PathBuf::from(other));
                index += 1;
            }
        }
    }

    let input = input.ok_or_else(usage)?;
    let output = output.ok_or_else(|| "--out обязателен".to_owned())?;
    shader::compile(&input, &output, &options).map_err(|error| error.to_string())?;
    println!("SPIR-V: {}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_without_command_is_an_error() {
        assert!(run(&[]).is_err());
    }

    #[test]
    fn unknown_command_is_rejected() {
        assert!(run(&["nope".to_owned()]).is_err());
    }

    #[test]
    fn shader_requires_output() {
        let args = vec![
            "shader".to_owned(),
            "in.hlsl".to_owned(),
            "--entry".to_owned(),
            "vs_main".to_owned(),
            "--stage".to_owned(),
            "vs_6_0".to_owned(),
        ];
        assert!(run(&args).is_err());
    }

    #[test]
    fn bad_define_is_rejected() {
        let args = vec![
            "shader".to_owned(),
            "in.hlsl".to_owned(),
            "--out".to_owned(),
            "out.spv".to_owned(),
            "--define".to_owned(),
            "BROKEN".to_owned(),
        ];
        assert!(run(&args).is_err());
    }
}
