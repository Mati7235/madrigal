use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use madrigal_app::{boot_headless, cli};
use madrigal_player::single_instance::{acquire, InstanceRole};

fn main() -> ExitCode {
    let opts = match cli::parse(std::env::args().skip(1)) {
        Ok(opts) => opts,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(2);
        }
    };
    let profile_dir = match cli::profile_dir(&opts) {
        Ok(dir) => dir,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::from(2);
        }
    };
    let _instance = match acquire() {
        Ok(InstanceRole::Primary(guard)) => guard,
        Ok(InstanceRole::Secondary) => {
            println!("madrigal ya está en ejecución");
            return ExitCode::SUCCESS;
        }
        Err(err) => {
            eprintln!("single-instance: {err}");
            return ExitCode::from(1);
        }
    };
    let boot = match boot_headless(&profile_dir) {
        Ok(boot) => boot,
        Err(err) => {
            eprintln!("arranque: {err}");
            return ExitCode::from(1);
        }
    };
    println!("READY {}", boot.startup_ms);
    if let Some(ms) = opts.smoke_quit_after_ms {
        thread::sleep(Duration::from_millis(ms));
    }
    ExitCode::SUCCESS
}
