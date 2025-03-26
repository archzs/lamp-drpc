pub use std::fs;
pub use std::io::Write;
pub use std::process;

pub fn log_error(etype: &str, e: &str) {
    eprintln!("{}: {}", &etype, &e);
    if let Some(home_path) = std::env::home_dir() {
        match home_path.to_str() {
            Some(no_unicode_path) => {
                let err_log_file_path = format!("{no_unicode_path}/.config/lamp-drpc/lamp-error.log");
                let err_log_file = fs::OpenOptions::new()
                                .read(false)
                                .write(true)
                                .create(true)
                                .append(true)
                                .open(err_log_file_path);

                match err_log_file { 
                    Ok(mut err_log_file) => {
                        let Ok(_) = write!(err_log_file, "[{}] {}: {}\n", chrono::offset::Local::now(), &etype, &e) else {
                            eprintln!("error_log:err_log_file write Error: {}", e);
                            process::exit(1);
                        };
                    }
                    Err(e) => {
                        eprintln!("error_log:err_log_file match Error: {}", e);
                        process::exit(1);
                    }
                }
            }
            None => {
                eprintln!("error_log:home_path.to_str() Error: Home directory path contains unicode characters.");
                process::exit(1);
            }
        }
    } else {
        eprintln!("error_log:home_dir() Error: Could not find home directory.");
        process::exit(1);
    }
}