use std::io::Read;
use std::path::Path;
use std::{env, fs, process};

use kooixc::error::{Diagnostic, Severity};
use kooixc::loader::{load_source_map, SourceMap};
use kooixc::native::NativeError;
use kooixc::{
    check_entry_modules, check_source, compile_and_run_native_source_with_args_stdin_and_timeout,
    compile_native_source, emit_llvm_ir_source, lower_source, lower_to_mir_source, parse_source,
    run_source, ModuleCheckResult,
};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        print_usage();
        process::exit(2);
    }

    let command = args[1].as_str();
    if command == "native-llvm" {
        let ll_path = Path::new(&args[2]);
        let options = match parse_native_options(&args[3..]) {
            Ok(options) => options,
            Err(message) => {
                eprintln!("{message}");
                print_usage();
                process::exit(2);
            }
        };

        let ir = match fs::read_to_string(ll_path) {
            Ok(ir) => ir,
            Err(error) => {
                eprintln!("failed to read llvm ir file {}: {error}", ll_path.display());
                process::exit(2);
            }
        };

        let output_path = Path::new(&options.output);
        match kooixc::native::compile_llvm_ir_to_executable(&ir, output_path) {
            Ok(()) => {
                println!("ok: native binary generated at {}", options.output);
            }
            Err(error) => {
                eprintln!("native build failed: {error}");
                process::exit(1);
            }
        }

        if options.run_after_build {
            let stdin_data = match options.stdin_path.as_deref() {
                Some("-") => {
                    let mut buffer = Vec::new();
                    if let Err(error) = std::io::stdin().read_to_end(&mut buffer) {
                        eprintln!("failed to read stdin: {error}");
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

            match kooixc::native::run_executable_with_args_and_stdin_and_timeout(
                output_path,
                &options.run_args,
                stdin_data.as_deref(),
                options.timeout_ms,
            ) {
                Ok(run_output) => {
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
                    eprintln!("native run failed: {error}");
                    process::exit(1);
                }
            }
        }

        return;
    }

    let file = &args[2];

    let entry_path = Path::new(file);
    if command == "check-modules" {
        let options = match parse_check_modules_options(&args[3..]) {
            Ok(options) => options,
            Err(message) => {
                eprintln!("{message}");
                print_usage();
                process::exit(2);
            }
        };

        match check_entry_modules(entry_path) {
            Ok(results) => {
                let has_diagnostics = results.iter().any(|result| !result.diagnostics.is_empty());
                if options.json {
                    print_module_diagnostics_json(&results);
                    if has_diagnostics {
                        process::exit(1);
                    }
                } else if !has_diagnostics {
                    println!("ok: module semantic checks passed");
                } else {
                    print_module_diagnostics(&results);
                    process::exit(1);
                }
            }
            Err(errors) => {
                if options.json {
                    print_loader_diagnostics_json(&errors);
                } else {
                    for error in errors {
                        eprintln!("error: {}", error.message);
                    }
                }
                process::exit(2);
            }
        }
        return;
    }

    let source_map = match load_source_map(entry_path) {
        Ok(map) => map,
        Err(errors) => {
            for error in errors {
                eprintln!("error: {}", error.message);
            }
            process::exit(2);
        }
    };
    let source = source_map.combined.as_str();

    match command {
        "check" => {
            let diagnostics = check_source(&source);
            if diagnostics.is_empty() {
                println!("ok: semantic checks passed");
            } else {
                print_diagnostics(&diagnostics, &source_map);
                process::exit(1);
            }
        }
        "ast" => match parse_source(&source) {
            Ok(program) => {
                println!("{program:#?}");
            }
            Err(errors) => {
                print_diagnostics(&errors, &source_map);
                process::exit(1);
            }
        },
        "hir" => match lower_source(&source) {
            Ok(program) => {
                println!("{program:#?}");
            }
            Err(errors) => {
                print_diagnostics(&errors, &source_map);
                process::exit(1);
            }
        },
        "mir" => match lower_to_mir_source(&source) {
            Ok(program) => {
                println!("{program:#?}");
            }
            Err(errors) => {
                print_diagnostics(&errors, &source_map);
                process::exit(1);
            }
        },
        "llvm" => match emit_llvm_ir_source(&source) {
            Ok(ir) => {
                println!("{ir}");
            }
            Err(errors) => {
                print_diagnostics(&errors, &source_map);
                process::exit(1);
            }
        },
        "run" => match run_source(&source) {
            Ok(result) => {
                if !result.diagnostics.is_empty() {
                    print_diagnostics(&result.diagnostics, &source_map);
                }
                println!("ok: run result: {}", result.value);
            }
            Err(errors) => {
                print_diagnostics(&errors, &source_map);
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
                        report_native_error(error, &source_map);
                        process::exit(1);
                    }
                }
            } else {
                match compile_native_source(&source, output_path) {
                    Ok(_) => {
                        println!("ok: native binary generated at {}", options.output);
                    }
                    Err(error) => {
                        report_native_error(error, &source_map);
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

fn print_diagnostics(diagnostics: &[Diagnostic], source_map: &SourceMap) {
    for diagnostic in diagnostics {
        let level = match diagnostic.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        if let Some(file) = source_map.locate(diagnostic.span.start) {
            let relative = diagnostic.span.start.saturating_sub(file.start);
            let (line, col) = byte_to_line_col(&file.source, relative);
            eprintln!(
                "{level}[{}:{line}:{col}]: {}",
                file.path.display(),
                diagnostic.message
            );
        } else {
            let (line, col) = byte_to_line_col(&source_map.combined, diagnostic.span.start);
            eprintln!("{level}[{line}:{col}]: {}", diagnostic.message);
        }
    }
}

fn print_module_diagnostics(results: &[ModuleCheckResult]) {
    for result in results {
        let module_source = fs::read_to_string(&result.path).ok();
        for diagnostic in &result.diagnostics {
            let level = match diagnostic.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            };

            if let Some(source) = &module_source {
                let start = diagnostic.span.start.min(source.len());
                let (line, col) = byte_to_line_col(source, start);
                eprintln!(
                    "{level}[{}:{line}:{col}]: {}",
                    result.path.display(),
                    diagnostic.message
                );
            } else {
                eprintln!("{level}[{}]: {}", result.path.display(), diagnostic.message);
            }
        }
    }
}

fn print_module_diagnostics_json(results: &[ModuleCheckResult]) {
    let mut out = String::new();
    out.push_str("{\"ok\":");
    out.push_str(
        if results.iter().all(|result| result.diagnostics.is_empty()) {
            "true"
        } else {
            "false"
        },
    );
    out.push_str(",\"modules\":[");

    for (module_index, result) in results.iter().enumerate() {
        if module_index > 0 {
            out.push(',');
        }

        out.push_str("{\"path\":\"");
        out.push_str(&escape_json_string(&result.path.display().to_string()));
        out.push_str("\",\"diagnostics\":[");
        for (diagnostic_index, diagnostic) in result.diagnostics.iter().enumerate() {
            if diagnostic_index > 0 {
                out.push(',');
            }

            out.push_str("{\"severity\":\"");
            out.push_str(diagnostic_severity_label(diagnostic.severity));
            out.push_str("\",\"message\":\"");
            out.push_str(&escape_json_string(&diagnostic.message));
            out.push_str("\",\"span\":{\"start\":");
            out.push_str(&diagnostic.span.start.to_string());
            out.push_str(",\"end\":");
            out.push_str(&diagnostic.span.end.to_string());
            out.push_str("}}");
        }
        out.push_str("]}");
    }

    out.push_str("]}");
    println!("{out}");
}

fn print_loader_diagnostics_json(errors: &[Diagnostic]) {
    let mut out = String::new();
    out.push_str("{\"ok\":false,\"phase\":\"load\",\"errors\":[");

    for (index, error) in errors.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"severity\":\"");
        out.push_str(diagnostic_severity_label(error.severity));
        out.push_str("\",\"message\":\"");
        out.push_str(&escape_json_string(&error.message));
        out.push_str("\",\"span\":{\"start\":");
        out.push_str(&error.span.start.to_string());
        out.push_str(",\"end\":");
        out.push_str(&error.span.end.to_string());
        out.push_str("}}");
    }

    out.push_str("]}");
    println!("{out}");
}

fn diagnostic_severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    }
}

fn escape_json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped
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
        "usage: kooixc <check|ast|hir|mir|llvm|run|native> <file.kooix> [output] [--run] [--stdin <file|-] [--timeout <ms>] [-- <args...>]\n       kooixc check-modules <file.kooix> [--json]\n       kooixc native-llvm <file.ll> [output] [--run] [--stdin <file|-] [--timeout <ms>] [-- <args...>]"
    );
}

fn report_native_error(error: NativeError, source_map: &SourceMap) {
    match error {
        NativeError::Diagnostics(diagnostics) => {
            print_diagnostics(&diagnostics, source_map);
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct CheckModulesOptions {
    json: bool,
}

fn parse_check_modules_options(args: &[String]) -> Result<CheckModulesOptions, String> {
    let mut json = false;

    for arg in args {
        if arg == "--json" {
            json = true;
            continue;
        }

        if arg.starts_with("--") {
            return Err(format!("unknown check-modules option '{arg}'"));
        }

        return Err(format!("unexpected check-modules argument '{arg}'"));
    }

    Ok(CheckModulesOptions { json })
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
    use super::{
        parse_check_modules_options, parse_native_options, CheckModulesOptions, NativeOptions,
    };

    #[test]
    fn parses_check_modules_defaults() {
        let args: Vec<String> = vec![];
        let options = parse_check_modules_options(&args).expect("should parse");
        assert_eq!(options, CheckModulesOptions { json: false });
    }

    #[test]
    fn parses_check_modules_json_option() {
        let args = vec!["--json".to_string()];
        let options = parse_check_modules_options(&args).expect("should parse");
        assert_eq!(options, CheckModulesOptions { json: true });
    }

    #[test]
    fn rejects_unknown_check_modules_option() {
        let args = vec!["--bad".to_string()];
        let error = parse_check_modules_options(&args).expect_err("should fail");
        assert!(error.contains("unknown check-modules option"));
    }

    #[test]
    fn rejects_unexpected_check_modules_argument() {
        let args = vec!["output.json".to_string()];
        let error = parse_check_modules_options(&args).expect_err("should fail");
        assert!(error.contains("unexpected check-modules argument"));
    }

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
