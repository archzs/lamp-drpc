use std::env;
use std::thread;
use std::time::Duration;
use serde::Deserialize;
use sysinfo::{Pid, ProcessStatus, ProcessesToUpdate, ProcessRefreshKind, RefreshKind, System};

mod error_log;
use error_log::fs;
use error_log::Write;
use error_log::process;

mod player;
use player::Cmus;
use player::StandardPlayer;
use player::Path;


enum MusicPlayer {
    Cmus(player::Cmus),
}

impl StandardPlayer for MusicPlayer {
    fn verify_running(&self) -> bool {
        match self {
            MusicPlayer::Cmus(cmus) => return Cmus::verify_running(&cmus),
            _ => {
                todo!();
            }
        }
    }

    fn get_active_file_path(&mut self) -> String {
        match self {
            MusicPlayer::Cmus(cmus) => return Cmus::get_active_file_path(cmus),
            _ => {
                todo!();
            }
        }
    }

    fn get_position_and_duration(&self) -> (Option<u32>, Option<u32>) {
        match self {
            MusicPlayer::Cmus(cmus) => return Cmus::get_position_and_duration(&cmus),
            _ => {
                todo!();
            }
        }
    }
}

#[derive(Deserialize)]
struct Config {
    player_name: String,
    player_check_delay: u64,
    run_secondary_checks: bool,
    va_individual_art: bool,
}

fn main() {
    let config_values: Config = load_config();
    let sleep_time: Duration = Duration::from_secs(config_values.player_check_delay);
    
    // Assign MusicPlayer type based on provided player_name
    let mut active_music_player: MusicPlayer;
    match config_values.player_name.as_str() {
        "cmus" => active_music_player = MusicPlayer::Cmus(Cmus::default()),
        _ => {
            error_log::log_error("Error", format!("The player_name \"{}\" provided in the lamp.toml configuration file is unsupported.", config_values.player_name).as_str());
            process::exit(1); 
        }
    }

    // Wait player_check_delay number of seconds before checking that player is running
    thread::sleep(sleep_time);

    // Instantiate system instance with variable to track player status
    let mut sys = System::new_with_specifics(RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing()));
    let mut player_status = ProcessStatus::Stop;

    // Get PID of player process for checking process status
    let player_pid = get_pid_by_proc_name(&sys, &config_values.player_name);

    // Get status of player process by PID
    player_status = get_status_by_pid(&sys, &player_pid);

    if config_values.run_secondary_checks {
        if !&active_music_player.verify_running() {
            error_log::log_error("Error", format!("Secondary check(s) failed for player {}.", config_values.player_name).as_str());
            process::exit(1);
        }
    }

    // Declare variables for use in main loop
    let mut active_file_path = String::new();   // The path of the currently playing track.
    let mut previous_file_path = String::new(); // The path of the previous track, used to determine when the active track has changed.
    let mut active_position_duration: (Option<u32>, Option<u32>) = (None, None); // The 

    // Begin main loop
    while player_status != ProcessStatus::Stop {
        // Update active file path, duration, and position
        active_file_path = active_music_player.get_active_file_path();
        active_position_duration = active_music_player.get_position_and_duration();

        // Refresh system to get updates to player process
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[player_pid]),
            true,
            ProcessRefreshKind::nothing(),
        );

        // Check player status, exit if None
        let Some(player_process) = sys.process(Pid::from(player_pid)) else {
            process::exit(0);
        };
        player_status = player_process.status();
        previous_file_path = active_file_path;
    }
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
                    error_log::log_error("toml Error", &e.message());
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
                - player_check_delay becomes the amount of time to sleep before checking for the player running.
                  Default is 5.
                - run_secondary_checks determines whether or not player-specific secondary verification of status
                  should be performed. Default is true.
                - va_individual_art indidcates whether or not tracks with "Various Artists" as the album artist
                  should have their album art processed individually. Default is true.
            */ 
            match config_file {
                Ok(_) =>  {
                    let _ = write!(config_file.expect("Configuration file should exist and be accessible at this point."), "player_name = \'cmus\'\n\
                                                                                                                            player_check_delay = 5\n\
                                                                                                                            run_secondary_checks = true\n\
                                                                                                                            va_individual_art = true\n");
                },
                Err(e) => {
                    error_log::log_error("Config Error", &e.to_string().as_str());
                    process::exit(1);
                },
            }

            let config_values = Config {
                player_name: String::from("cmus"),
                player_check_delay: 5,
                run_secondary_checks: true,
                va_individual_art: true,
            };
            return config_values;
        },
        Err(e) => { 
            error_log::log_error("Error", &e.to_string().as_str());
            process::exit(1); 
        },
    }
}

/* fn log_error(e: &str) {
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
} */

fn get_pid_by_proc_name(sys: &System, proc_name: &String) -> sysinfo::Pid {
    if let Some(possible_process) = sys.processes_by_exact_name(proc_name.as_ref()).next() {
        return possible_process.pid();
    }
    else {
        error_log::log_error("Error", format!("The PID of target player {} could not be determined. The player may not be running or may have a different process name than provided in the configuration file.", proc_name).as_str());
        process::exit(1);
    }
}

fn get_status_by_pid(sys: &System, player_pid: &sysinfo::Pid) -> ProcessStatus {
    if let Some(player_process) = sys.process(*player_pid) {
        return player_process.status();
    }
    else {
        error_log::log_error("Error", "The target PID could not be found. The player may no longer be running.");
        process::exit(1);
    }
}