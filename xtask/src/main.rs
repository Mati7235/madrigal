use std::process::ExitCode;

use xtask::{discover, execute, find_registry, load_registry, parse_args, select, Runner};

const USAGE: &str = "uso: cargo xtask checks [--group offline|native|live|visual] [--only <check>[,<check>...]] [--crate <crate>] [--max-fails N]";

fn main() -> ExitCode {
    let mut argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().is_some_and(|a| a == "xtask") {
        argv.remove(0);
    }
    if argv.first().is_none_or(|a| a != "checks") {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let opts = match parse_args(&argv[1..]) {
        Ok(opts) => opts,
        Err(err) => {
            eprintln!("{err}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let registry = find_registry();
    let root = registry.parent().unwrap_or(std::path::Path::new("."));
    let checks = load_registry(&registry);
    let inventory = discover(root);
    let selected = select(&checks, &opts, &inventory);
    let mut runner = Runner::new(root, &selected);
    let (lines, code) = execute(&selected, opts.max_fails, &mut |selection| {
        runner.run(selection)
    });
    for line in lines {
        println!("{line}");
    }
    ExitCode::from(code as u8)
}
