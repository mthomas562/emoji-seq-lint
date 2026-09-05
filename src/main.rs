use std::env;
use std::fs;
use std::process::ExitCode;

use emoji_seq_lint::{classify, find_duplicates, parse};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let command = match args.next() {
        Some(c) => c,
        None => {
            print_usage();
            return ExitCode::FAILURE;
        }
    };

    match command.as_str() {
        "check" => {
            let path = match args.next() {
                Some(p) => p,
                None => {
                    eprintln!("error: 'check' requires a file path");
                    return ExitCode::FAILURE;
                }
            };
            run_check(&path)
        }
        "help" | "--help" | "-h" => {
            print_usage();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("error: unknown command '{}'", other);
            print_usage();
            ExitCode::FAILURE
        }
    }
}

fn run_check(path: &str) -> ExitCode {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read '{}': {}", path, e);
            return ExitCode::FAILURE;
        }
    };

    let (entries, mut diagnostics) = parse(&source);

    for entry in &entries {
        if let Err(diag) = classify(entry) {
            diagnostics.push(diag);
        }
    }
    diagnostics.extend(find_duplicates(&entries));

    if diagnostics.is_empty() {
        println!("{}: {} entries, no errors", path, entries.len());
        return ExitCode::SUCCESS;
    }

    diagnostics.sort_by_key(|d| d.line);
    for diag in &diagnostics {
        print!("{}", diag.render(path, &source));
    }
    eprintln!(
        "{} error{} in {}",
        diagnostics.len(),
        if diagnostics.len() == 1 { "" } else { "s" },
        path
    );
    ExitCode::FAILURE
}

fn print_usage() {
    eprintln!("usage: emojiseq check <file>");
}
