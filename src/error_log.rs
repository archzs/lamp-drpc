pub use std::fs;
pub use std::io::Write;
pub use std::process;

pub fn log_error(etype: &str, e: &str) {
    eprintln!("{}: {}", &etype, &e);
    match std::env::home_dir() {
        Some(path) => {
            let config_dir_path = path.to_str().unwrap().to_owned() + "/.config/lamp-drpc";
            let err_log_file_path = config_dir_path + "/lamp-error.log";
            let err_log_file = fs::OpenOptions::new()
                                .read(false)
                                .write(true)
                                .create(true)
                                .append(true)
                                .open(err_log_file_path);
            match err_log_file {
                Ok(_) =>  {
                    let _ = write!(err_log_file.expect("Error log file should exist and be accessible at this point."), "[{}] {}: {}\n", chrono::offset::Local::now(), &etype, &e);
                },
                Err(e) => {
                    eprintln!("Error: {}", e);
                    process::exit(1);
                },
            }
        },
        None => {
            eprintln!("Error: Could not find home directory.");
            process::exit(1);
        },
    } 
}