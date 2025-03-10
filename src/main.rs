use std::env;
use std::fs;
use std::path::Path;
use std::process;
use std::io::Write;
use serde::Deserialize;

#[derive(Deserialize)]
struct Config {
    player_name: String,
    va_individual_art: bool,
}

fn main() {
    let config_values: Config = load_config();
    println!("Player name: {}", config_values.player_name);
    // Add error logging on errors
    todo!();
}

fn load_config() -> Config {
    // Check if config directory exists at $HOME/.config/lamp-drpc, create it if it does not.
    let config_dir_path;
    match env::home_dir() {
        Some(path) => {
            config_dir_path = path.to_str().unwrap().to_owned() + "/.config/lamp-drpc";
        },
        None => {
            eprintln!("Error: Could not find home directory.");
            process::exit(1);
        },
    }
    match fs::exists(&config_dir_path) {
        Ok(true) if Path::new(&config_dir_path.as_str()).is_dir() => {},
        Ok(true) => { 
            // File exists at configuration directory path, but is not a directory.
            eprintln!("Error: file at {} is not a directory.", config_dir_path);
            process::exit(1);
        },
        Ok(false) => {
            // Configuration directory does not exist, create it now.
            match fs::create_dir_all(&config_dir_path) {
                Ok(_) => {},
                Err(e) =>  {
                    eprintln!("Error: {}", e);
                    process::exit(1);
                },
            }
        },
        Err(e) => { 
            eprintln!("Error: {}", e); 
            process::exit(1); 
        },
    }
    
    // Check for configuration file. If it exists, read it, otherwise, create with default values.
    let config_file_path = config_dir_path + "/lamp.toml";
    match fs::exists(&config_file_path) {
        Ok(true) => {
            // Read existing configuration file.         
            let toml_string = fs::read_to_string(config_file_path).ok().unwrap_or(String::from("player_name = \'cmus\'"));
            match toml::from_str(toml_string.as_str()) {
                Ok(config_values) => return config_values,
                Err(e) => {
                    log_error(&e.message());
                    process::exit(1);
                }
            }
        },
        Ok(false) => {
            // Configuration file does not exist, create it now and write default values to it.
            let config_file = fs::OpenOptions::new()
                                .read(false)
                                .write(true)
                                .create(true)
                                .open(config_file_path);
            
            /* 
                Set default configuration values.
                - player_name is the name of the process to be tracked while running. Default is 'cmus'.
                - va_individual_art indidcates whether or not tracks with "Various Artists" as the album artist
                  should have their album art processed individually. Default is true.
            */ 
            match config_file {
                Ok(_) =>  {
                    let _ = write!(config_file.expect("Configuration file should exist and be accessible at this point."), "player_name = \'cmus\'\n\
                                                                                                                            va_individual_art = true\n");
                },
                Err(e) => {
                    log_error(&e.to_string().as_str());
                    process::exit(1);
                },
            }

            let config_values = Config {
                player_name: String::from("cmus"),
                va_individual_art: true,
            };
            return config_values;
        },
        Err(e) => { 
            log_error(&e.to_string().as_str());
            process::exit(1); 
        },
    }
}

fn log_error(e: &str) {
    eprintln!("Error: {}", &e);
    match env::home_dir() {
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
                    let _ = write!(err_log_file.expect("Error log file should exist and be accessible at this point."), "[{}] Error: {}\n", chrono::offset::Local::now(), &e);
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