use std::io::Read;
use std::path::Path;
use std::{env, fs, process};

use kooixc::error::{Diagnostic, Severity};
use kooixc::native::NativeError;
use kooixc::{
    check_source, compile_and_run_native_source_with_args_stdin_and_timeout, compile_native_source,
    emit_llvm_ir_source, lower_source, lower_to_mir_source, parse_source,
};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        print_usage();
        process::exit(2);
    }

    let command = args[1].as_str();
    let file = &args[2];

    let source = match fs::read_to_string(file) {
        Ok(content) => content,
        Err(error) => {
            eprintln!("failed to read {file}: {error}");
            process::exit(2);
        }
    };

    match command {
        "check" => {
            let diagnostics = check_source(&source);
            if diagnostics.is_empty() {
                println!("ok: semantic checks passed");
            } else {
                print_diagnostics(&diagnostics, &source);
                process::exit(1);
            }
        }
        "ast" => match parse_source(&source) {
            Ok(program) => {
                println!("{program:#?}");
            }
            Err(errors) => {
                print_diagnostics(&errors, &source);
                process::exit(1);
            }
        },
        "hir" => match lower_source(&source) {
            Ok(program) => {
                println!("{program:#?}");
            }
            Err(errors) => {
                print_diagnostics(&errors, &source);
                process::exit(1);
            }
        },
        "mir" => match lower_to_mir_source(&source) {
            Ok(program) => {
                println!("{program:#?}");
            }
            Err(errors) => {
                print_diagnostics(&errors, &source);
                process::exit(1);
            }
        },
        "llvm" => match emit_llvm_ir_source(&source) {
            Ok(ir) => {
                println!("{ir}");
            }
            Err(errors) => {
                print_diagnostics(&errors, &source);
                process::exit(1);
            }
        },
        "native" => {
            let options = match parse_native_options(&args[3..]) {
                Ok(options) => options,
                Err(message) => {
                    eprintln!("{message}");
                    print_usage();
                    process::exit(2);
                }
            };

            let output_path = Path::new(&options.output);
            if options.run_after_build {
                let stdin_data = match &options.stdin_path {
                    Some(path) if path == "-" => {
                        let mut buffer = Vec::new();
                        if let Err(error) = std::io::stdin().read_to_end(&mut buffer) {
                            eprintln!("failed to read stdin stream: {error}");
                            process::exit(2);
                        }
                        Some(buffer)
                    }
                    Some(path) => match fs::read(path) {
                        Ok(data) => Some(data),
                        Err(error) => {
                            eprintln!("failed to read stdin file {path}: {error}");
                            process::exit(2);
                        }
                    },
                    None => None,
                };

                match compile_and_run_native_source_with_args_stdin_and_timeout(
                    &source,
                    output_path,
                    &options.run_args,
                    stdin_data.as_deref(),
                    options.timeout_ms,
                ) {
                    Ok(run_output) => {
                        println!("ok: native binary generated at {}", options.output);
                        if !run_output.stdout.is_empty() {
                            print!("{}", run_output.stdout);
                        }
                        if !run_output.stderr.is_empty() {
                            eprint!("{}", run_output.stderr);
                        }
                        let exit_code = run_output.status_code.unwrap_or(1);
                        println!("run exit code: {exit_code}");
                        if exit_code != 0 {
                            process::exit(exit_code);
                        }
                    }
                    Err(error) => {
                        report_native_error(error, &source);
                        process::exit(1);
                    }
                }
            } else {
                match compile_native_source(&source, output_path) {
                    Ok(_) => {
                        println!("ok: native binary generated at {}", options.output);
                    }
                    Err(error) => {
                        report_native_error(error, &source);
                        process::exit(1);
                    }
                }
            }
        }
        _ => {
            print_usage();
            process::exit(2);
        }
    }
}

fn print_diagnostics(diagnostics: &[Diagnostic], source: &str) {
    for diagnostic in diagnostics {
        let (line, col) = byte_to_line_col(source, diagnostic.span.start);
        let level = match diagnostic.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        eprintln!("{level}[{line}:{col}]: {}", diagnostic.message);
    }
}

fn byte_to_line_col(source: &str, byte_index: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (index, ch) in source.char_indices() {
        if index >= byte_index {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn print_usage() {
    eprintln!(
        "usage: kooixc <check|ast|hir|mir|llvm|native> <file.aster> [output] [--run] [--stdin <file|-] [--timeout <ms>] [-- <args...>]"
    );
}

fn report_native_error(error: NativeError, source: &str) {
    match error {
        NativeError::Diagnostics(diagnostics) => {
            print_diagnostics(&diagnostics, source);
        }
        other => {
            eprintln!("native build failed: {other}");
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeOptions {
    output: String,
    run_after_build: bool,
    run_args: Vec<String>,
    stdin_path: Option<String>,
    timeout_ms: Option<u64>,
}

fn parse_native_options(args: &[String]) -> Result<NativeOptions, String> {
    let mut output: Option<String> = None;
    let mut run_after_build = false;
    let mut run_args = Vec::new();
    let mut stdin_path: Option<String> = None;
    let mut timeout_ms: Option<u64> = None;
    let mut parse_run_args = false;
    let mut expect_stdin_path = false;
    let mut expect_timeout_ms = false;

    for arg in args {
        if expect_stdin_path {
            stdin_path = Some(arg.clone());
            expect_stdin_path = false;
            continue;
        }

        if expect_timeout_ms {
            let parsed = arg
                .parse::<u64>()
                .map_err(|_| format!("invalid --timeout value '{arg}'"))?;
            timeout_ms = Some(parsed);
            expect_timeout_ms = false;
            continue;
        }

        if parse_run_args {
            run_args.push(arg.clone());
            continue;
        }

        if arg == "--" {
            parse_run_args = true;
            continue;
        }

        if arg == "--run" {
            run_after_build = true;
            continue;
        }

        if arg == "--stdin" {
            expect_stdin_path = true;
            continue;
        }

        if arg == "--timeout" {
            expect_timeout_ms = true;
            continue;
        }

        if arg.starts_with("--") {
            return Err(format!("unknown native option '{arg}'"));
        }

        if output.is_some() {
            return Err("multiple native output paths provided".to_string());
        }

        output = Some(arg.clone());
    }

    if expect_stdin_path {
        return Err("missing value for --stdin".to_string());
    }

    if expect_timeout_ms {
        return Err("missing value for --timeout".to_string());
    }

    if stdin_path.is_some() && !run_after_build {
        return Err("--stdin requires --run".to_string());
    }

    if timeout_ms.is_some() && !run_after_build {
        return Err("--timeout requires --run".to_string());
    }

    Ok(NativeOptions {
        output: output.unwrap_or_else(|| "a.out".to_string()),
        run_after_build,
        run_args,
        stdin_path,
        timeout_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::{parse_native_options, NativeOptions};

    #[test]
    fn parses_native_defaults() {
        let args: Vec<String> = vec![];
        let options = parse_native_options(&args).expect("should parse");
        assert_eq!(
            options,
            NativeOptions {
                output: "a.out".to_string(),
                run_after_build: false,
                run_args: vec![],
                stdin_path: None,
                timeout_ms: None,
            }
        );
    }

    #[test]
    fn parses_native_run_with_output_and_args() {
        let args = vec![
            "target/demo".to_string(),
            "--run".to_string(),
            "--".to_string(),
            "alpha".to_string(),
            "beta".to_string(),
        ];
        let options = parse_native_options(&args).expect("should parse");
        assert_eq!(
            options,
            NativeOptions {
                output: "target/demo".to_string(),
                run_after_build: true,
                run_args: vec!["alpha".to_string(), "beta".to_string()],
                stdin_path: None,
                timeout_ms: None,
            }
        );
    }

    #[test]
    fn parses_native_run_with_stdin() {
        let args = vec![
            "target/demo".to_string(),
            "--run".to_string(),
            "--stdin".to_string(),
            "input.txt".to_string(),
            "--".to_string(),
            "alpha".to_string(),
        ];
        let options = parse_native_options(&args).expect("should parse");
        assert_eq!(
            options,
            NativeOptions {
                output: "target/demo".to_string(),
                run_after_build: true,
                run_args: vec!["alpha".to_string()],
                stdin_path: Some("input.txt".to_string()),
                timeout_ms: None,
            }
        );
    }

    #[test]
    fn parses_native_run_with_stdin_stream() {
        let args = vec![
            "target/demo".to_string(),
            "--run".to_string(),
            "--stdin".to_string(),
            "-".to_string(),
            "--".to_string(),
            "alpha".to_string(),
        ];
        let options = parse_native_options(&args).expect("should parse");
        assert_eq!(
            options,
            NativeOptions {
                output: "target/demo".to_string(),
                run_after_build: true,
                run_args: vec!["alpha".to_string()],
                stdin_path: Some("-".to_string()),
                timeout_ms: None,
            }
        );
    }

    #[test]
    fn parses_native_run_with_timeout() {
        let args = vec![
            "target/demo".to_string(),
            "--run".to_string(),
            "--timeout".to_string(),
            "250".to_string(),
            "--".to_string(),
            "alpha".to_string(),
        ];
        let options = parse_native_options(&args).expect("should parse");
        assert_eq!(
            options,
            NativeOptions {
                output: "target/demo".to_string(),
                run_after_build: true,
                run_args: vec!["alpha".to_string()],
                stdin_path: None,
                timeout_ms: Some(250),
            }
        );
    }

    #[test]
    fn rejects_unknown_native_option() {
        let args = vec!["--bad".to_string()];
        let error = parse_native_options(&args).expect_err("should fail");
        assert!(error.contains("unknown native option"));
    }

    #[test]
    fn rejects_multiple_output_paths() {
        let args = vec!["a.out".to_string(), "b.out".to_string()];
        let error = parse_native_options(&args).expect_err("should fail");
        assert!(error.contains("multiple native output paths"));
    }

    #[test]
    fn rejects_stdin_without_run() {
        let args = vec!["--stdin".to_string(), "input.txt".to_string()];
        let error = parse_native_options(&args).expect_err("should fail");
        assert!(error.contains("--stdin requires --run"));
    }

    #[test]
    fn rejects_stdin_missing_value() {
        let args = vec!["--run".to_string(), "--stdin".to_string()];
        let error = parse_native_options(&args).expect_err("should fail");
        assert!(error.contains("missing value for --stdin"));
    }

    #[test]
    fn rejects_timeout_without_run() {
        let args = vec!["--timeout".to_string(), "200".to_string()];
        let error = parse_native_options(&args).expect_err("should fail");
        assert!(error.contains("--timeout requires --run"));
    }

    #[test]
    fn rejects_timeout_missing_value() {
        let args = vec!["--run".to_string(), "--timeout".to_string()];
        let error = parse_native_options(&args).expect_err("should fail");
        assert!(error.contains("missing value for --timeout"));
    }

    #[test]
    fn rejects_timeout_invalid_value() {
        let args = vec![
            "--run".to_string(),
            "--timeout".to_string(),
            "oops".to_string(),
        ];
        let error = parse_native_options(&args).expect_err("should fail");
        assert!(error.contains("invalid --timeout value"));
    }
}
