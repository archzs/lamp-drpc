pub use std::path::Path;

use crate::error_log;
use crate::error_log::fs;
use crate::error_log::process;

// MusicPlayer includes definitions for standard functionality, including filename retrieval and secondary status verification.
pub trait StandardPlayer {
    fn verify_running(&self) -> bool;
    fn get_active_file_path(&mut self) -> String;
    fn get_position_and_duration(&self) -> (Option<u32>, Option<u32>);
}

pub struct Cmus {
    pub cmus_remote_output: String,
    active_position_duration: (Option<u32>, Option<u32>),
}

impl Default for Cmus {
    fn default() -> Self {
        Cmus {
            cmus_remote_output: String::new(),
            active_position_duration: (None, None),
        }
    }
}

impl Cmus {
    fn update_cmus_remote_output() -> String {
        // Get info about current track from cmus-remote.
        let cmus_remote_output = process::Command::new("cmus-remote")
                                                                .arg("-Q")
                                                                .output();
        
        // If output returns an error, log and exit. Otherwise, attempt to process string.
        if cmus_remote_output.is_err() {
            error_log::log_error("Error", cmus_remote_output.unwrap_err().to_string().as_str());
            process::exit(1);
        } else {
            let remote_output_string = String::from_utf8(cmus_remote_output.unwrap().stdout);
            match remote_output_string {
                Ok(ok_string) => ok_string,
                Err(utf_error) => {
                    error_log::log_error("UTF-8 Error", utf_error.to_string().as_str());
                    process::exit(1);
                }
            }
        }
    }
}

impl StandardPlayer for Cmus {
    fn verify_running(&self) -> bool {
        // If cmus-socket exists and is not a directory/symlink, secondary check is passed.
        match fs::exists("/run/user/1000/cmus-socket") {
            Ok(true) if !Path::new("/run/user/1000/cmus-socket").is_dir() => true,
            Ok(true) => { 
                // File exists, but is a directory.
                error_log::log_error("Error", "File at /run/user/1000/cmus-socket is not a normal file. It may be a directory or was unaccessible.");
                false
            },
            Ok(false) => false,
            Err(io_error) => {
                error_log::log_error("Error", io_error.to_string().as_str());
                process::exit(1);
            }
        }
    }

    fn get_active_file_path(&mut self) -> String {
        // Update output from cmus-remote, along with position and duration.
        // This update will always occur before position and duration is requested in the main loop, so those values will always be up to date.
        self.cmus_remote_output = Cmus::update_cmus_remote_output();
        let output_string_lines = self.cmus_remote_output.split('\n').collect::<Vec<&str>>();

        let active_file_path: String;
        let active_file_duration: Option<&str>;
        let active_file_position: Option<&str>;

        // Check the status reported by cmus-remote.
        match output_string_lines[0] {
            // If playing, paused, or stopped, read file path, duration, and position as normal from output.
            // If file path cannot be parsed, log error and exit.
            "status playing" | "status paused" | "status stopped" => {
                match output_string_lines[1].strip_prefix("file ") {
                    Some(file_path) => active_file_path = file_path.to_string(),
                    None => {
                        error_log::log_error("Error", format!("First line of cmus-remote output is unrecognized:\n\t{}", output_string_lines[0]).as_str());
                        process::exit(1);
                    }
                };
                active_file_duration = output_string_lines[2].strip_prefix("duration ");
                active_file_position = output_string_lines[3].strip_prefix("position ");
            },
            &_ => {
                error_log::log_error("Error", format!("First line of cmus-remote output is unrecognized:\n\t{}", output_string_lines[0]).as_str());
                process::exit(1);
            }
        }

        // Check str options. If duration and position could not be parsed, set to None. 
        match active_file_duration.unwrap_or_default().parse::<u32>() {
            Ok(duration) => self.active_position_duration.1 = Some(duration),
            Err(parse_error) => {
                error_log::log_error("Parse Int Error", parse_error.to_string().as_str());
                self.active_position_duration.1 = None;
            }
        }

        match active_file_position.unwrap_or_default().parse::<u32>() {
            Ok(position) => self.active_position_duration.0 = Some(position),
            Err(parse_error) => {
                error_log::log_error("Parse Int Error", parse_error.to_string().as_str());
                self.active_position_duration.0 = None;
            }
        }

        active_file_path
    }

    fn get_position_and_duration(&self) -> (Option<u32>, Option<u32>) {
        self.active_position_duration
    }
}