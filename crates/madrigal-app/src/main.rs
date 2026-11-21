use std::process::ExitCode;
use std::time::Duration;

use madrigal_app::cli;
use madrigal_app::state::AppState;
use madrigal_player::single_instance::{acquire, InstanceRole};
use slint::ComponentHandle;

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
    if !profile_dir.as_os_str().is_empty() {
        std::env::set_var(madrigal_paths::PROFILE_ROOT_ENV, &profile_dir);
    }
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
    let app = match AppState::boot() {
        Ok(app) => app,
        Err(err) => {
            eprintln!("arranque: {err}");
            return ExitCode::from(1);
        }
    };
    if !opts.files.is_empty() {
        let (added, errors) = app.open_files(&opts.files);
        for err in &errors {
            eprintln!("archivo: {err}");
        }
        if added == 0 {
            eprintln!("no se pudo encolar ningún archivo");
            return ExitCode::from(2);
        }
    }
    if let Some(ms) = opts.smoke_quit_after_ms {
        slint::Timer::single_shot(Duration::from_millis(ms), || {
            let _ = slint::quit_event_loop();
        });
        if let Err(err) = slint::run_event_loop() {
            eprintln!("event loop: {err}");
            return ExitCode::from(1);
        }
        println!("READY");
        return ExitCode::SUCCESS;
    }
    if let Err(err) = app.ui.shell.show() {
        eprintln!("ventana: {err}");
        return ExitCode::from(1);
    }
    if let Err(err) = slint::run_event_loop() {
        eprintln!("event loop: {err}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
