pub use std::path::Path;

use crate::error_log;
use crate::error_log::fs;
use crate::error_log::process;

/* 
 *  StandardPlayer includes definitions for standard functionality, including filename retrieval and secondary status verification.
 * 
 *  - Only get_active_file_path is required to have an actual implementation for minimum functionality (Showing metadata for a current track on Discord).
 * 
 *  - verify_running is only used as a secondary check in addition to referencing a player's PID, to be really sure that the player is running properly.
 *    If no such secondary check is desired, this function should simply return true.
 *  
 *  - Implementing get_duration will enable the display of a progress bar on Discord's rich presence in addition to the metadata.
 */
pub trait StandardPlayer {
    fn verify_running(&self) -> bool;
    fn get_active_file_path(&mut self) -> Result<Option<String>, Box<dyn std::error::Error>>;
    fn get_duration(&self) -> Option<u64>;
}

/************************** Function Implementations for cmus **************************/
pub struct Cmus {
    pub cmus_remote_output: Option<String>,
    active_duration: Option<u64>,
}

impl Default for Cmus {
    fn default() -> Self {
        Cmus {
            cmus_remote_output: Some(String::new()),
            active_duration: None,
        }
    }
}

impl Cmus {
    fn update_cmus_remote_output() -> Option<String> {
        // Get info about current track from cmus-remote.
        let cmus_remote_output = process::Command::new("cmus-remote")
                                                                .arg("-Q")
                                                                .output();
        
        // If output returns an error, log and exit. Otherwise, attempt to process string.
        match cmus_remote_output {
            Ok(output) => {
                match String::from_utf8(output.stdout) {
                    Ok(ok_string) => Some(ok_string),
                    Err(e) => {
                        error_log::log_error("UTF-8 Error", e.to_string().as_str());
                        return None;
                    }
                }
            }
            Err(e) => {
                error_log::log_error("player:Cmus:update_cmus_remote_output Error", e.to_string().as_str());
                process::exit(1);
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
                error_log::log_error("player:Cmus:verify_running Error", "File at /run/user/1000/cmus-socket is not a normal file. It may be a directory or was unaccessible.");
                false
            },
            Ok(false) => false,
            Err(io_error) => {
                error_log::log_error("player:Cmus:verify_running Error", io_error.to_string().as_str());
                process::exit(1);
            }
        }
    }

    fn get_active_file_path(&mut self) -> Result<Option<String>, Box<dyn std::error::Error>> {
        // Update output from cmus-remote, along with position and duration.
        // This update will always occur before position and duration is requested in the main loop, so those values will always be up to date.
        self.cmus_remote_output = Cmus::update_cmus_remote_output();
        match &self.cmus_remote_output  {
            Some(cmus_remote_output) => {
                let output_string_lines = cmus_remote_output.split('\n').collect::<Vec<&str>>();

                let active_file_path: Option<String>;
                let active_file_duration: Option<&str>;
                //let active_file_position: Option<&str>;

                // Check the status reported by cmus-remote.
                match output_string_lines[0] {
                    // If playing, paused, or stopped, read file path, duration, and position as normal from output.
                    // If file path cannot be parsed, log error and exit.
                    "status playing" | "status paused" | "status stopped" => {
                        match output_string_lines[1].strip_prefix("file ") {
                            Some(file_path) => {
                                active_file_path = Some(file_path.to_string());
                                active_file_duration = output_string_lines[2].strip_prefix("duration ");
                            },
                            None => {
                                active_file_path = None;
                                active_file_duration = None;
                            }
                        };
                        
                        //active_file_position = output_string_lines[3].strip_prefix("position ");
                    },
                    &_ => return Err(Box::from("cmus has exited.")),
                }

                // Check str options. If duration and position could not be parsed, set to None. 
                self.active_duration = match active_file_duration.unwrap_or_default().parse::<u64>() {
                    Ok(duration) => Some(duration),
                    Err(_) => None,
                };

                /* match active_file_position.unwrap_or_default().parse::<u64>() {
                    Ok(position) => self.active_position_duration.0 = Some(position),
                    Err(e) => {
                        error_log::log_error("player:Cmus:get_active_file_path Error", e.to_string().as_str());
                        self.active_position_duration.0 = None;
                    }
                } */

                Ok(active_file_path)
            }
            None => Ok(None)
        }
    }

    fn get_duration(&self) -> Option<u64> {
        self.active_duration
    }
}
/************************** END Function Implementations for cmus **************************/

/************************** Function Implementations Template **************************/
/*

[PLAYER IMPLEMENTATION HERE]

// If a player struct will require data values for use in its functions, they should be included here
// and in the Default trait implementation.

pub struct NewPlayer {
    active_duration: Option<u64>,
}

impl Default for NewPlayer {
    fn default() -> Self {
        NewPlayer {
            active_duration: None,
        }
    }
}

impl StandardPlayer for NewPlayer {
    fn verify_running(&self) -> bool {
        return true;
    }

    // If None is returned, nothing will be shown on Discord, but the program will continue running.
    // Error logging is handled in main.rs when this function is called, so an error returned here will be logged.
    fn get_active_file_path(&mut self) -> Result<Option<String>, Box<dyn std::error::Error>> {
        Ok(None)
    }

    // active_duration does not need to be stored in the struct.
    fn get_duration(&self) -> Option<u64> {
        self.active_duration
    }
} 

*/
/************************** END Function Implementations Template **************************/