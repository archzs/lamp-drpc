use std::env;
use std::collections::HashMap;
use std::fs::{remove_file, File};
use std::io::{BufReader, Cursor};
use std::thread;
use std::time::Duration;
use discord_presence::models::{ActivityTimestamps, ActivityType};
use serde::Deserialize;
use serde_json::{Deserializer, Serializer, Value};
use sysinfo::{Pid, ProcessStatus, ProcessesToUpdate, ProcessRefreshKind, RefreshKind, System};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{DynamicImage, ImageEncoder, ImageFormat, ImageReader};
use std::io::BufWriter;
use fast_image_resize::images::Image;
use fast_image_resize::{IntoImageView, Resizer, ResizeOptions};
use http::{request, Method, StatusCode};
use std::time::{SystemTime, UNIX_EPOCH};
use catbox::file::from_file;

mod error_log;
use error_log::fs;
use error_log::Write;
use error_log::process;

mod player;
use player::Cmus;
use player::StandardPlayer;
use player::Path;

mod metadata;
use metadata::AlbumArt;
use metadata::MetadataPackage;
use metadata::read_metadata;

enum MusicPlayer {
    Cmus(player::Cmus),
}

impl StandardPlayer for MusicPlayer {
    fn verify_running(&self) -> bool {
        match self {
            MusicPlayer::Cmus(cmus) => return Cmus::verify_running(&cmus),
        }
    }

    fn get_active_file_path(&mut self) -> String {
        match self {
            MusicPlayer::Cmus(cmus) => return Cmus::get_active_file_path(cmus),
        }
    }

    fn get_position_and_duration(&self) -> (Option<u64>, Option<u64>) {
        match self {
            MusicPlayer::Cmus(cmus) => return Cmus::get_position_and_duration(&cmus),
        }
    }
}

#[derive(Deserialize)]
struct Config {
    player_name: String,
    player_check_delay: u64,
    run_secondary_checks: bool,
    va_album_individual: bool,
    catbox_user_hash: Option<String>,
}

fn main() {
    // Load configuration values from config file.
    let config_values: Config = match load_config() {
        Ok(config_values) => config_values,
        Err(e) => {
            error_log::log_error("main:load_config Error", e.to_string().as_str());
            process::exit(1);
        }
    };

    // Load HashMap from list stored in hash file.
    let mut filename_hash = match load_hash_file() {
        Ok(filename_hash) => filename_hash,
        Err(e) => {
            error_log::log_error("main:load_hash_file Error", e.to_string().as_str());
            process::exit(1);
        }
    };
    let sleep_time: Duration = Duration::from_secs(config_values.player_check_delay);
    let rest_time: Duration = Duration::from_millis(5);

    // Assign MusicPlayer type based on provided player_name
    let mut active_music_player: MusicPlayer;
    match config_values.player_name.as_str() {
        "cmus" => active_music_player = MusicPlayer::Cmus(Cmus::default()),
        _ => {
            error_log::log_error("main: active_music_player match Error", format!("The player_name \"{}\" provided in the lamp.toml configuration file is unsupported.", config_values.player_name).as_str());
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
    let mut active_file_image_link = Some(String::new()); // Link to the album art of the currently playing track, hosted on Imgur.
    let mut active_position_duration: (Option<u64>, Option<u64>) = (None, None); // The position of current playback and duration of audio file.
    let mut metadata_pack = MetadataPackage::default();
    let http_client = reqwest::Client::new();
    let mut discord_client = discord_presence::Client::new(1353193853393571910);
    discord_client.start();
    thread::sleep(sleep_time);

    // Begin main loop
    while player_status != ProcessStatus::Stop {
        // Update active file path, duration, and position
        active_file_path = active_music_player.get_active_file_path();
        active_position_duration = active_music_player.get_position_and_duration();

        // If active file has changed, then read metadata of new file.
        if active_file_path != previous_file_path {
            // Create and fill new MetadataPackage.
            metadata_pack = read_metadata(&active_file_path, &config_values.va_album_individual).unwrap();

            /* match discord_client.clear_activity() {
                Ok(_) => (),
                Err(e) => {
                    error_log::log_error("main: Discord Error on album art update", e.to_string().as_str());
                }
            } */

            // Check if Imgur information is defined in config file.
            // clientId known to be defined, since an error would have already been thrown otherwise.
            if config_values.catbox_user_hash.is_some() {
                match metadata_pack.album_art {
                    Some(album_art) => {
                        match filename_hash.get(&album_art.filename) {
                            // Filename exists in hash map.
                            Some(image_link) => {
                                // Link corresponding to filename is good, assign it to variable.
                                /* let link_status_good = match trpl::run(get_link_status(&http_client, image_link)) {
                                    Ok(link_status) => link_status,
                                    Err(e) => {
                                        error_log::log_error("main: link_status_good Error", e.to_string().as_str());
                                        false
                                    }
                                }; */
                                
                                if true {
                                    active_file_image_link = Some(image_link.clone());
                                } else { // Link is bad, upload again and update in hash map.
                                    // Clear current rich presence information so not visible while uploading.
                                    match discord_client.clear_activity() {
                                        Ok(_) => (),
                                        Err(e) => {
                                            error_log::log_error("main: Discord Error on album art update", e.to_string().as_str());
                                        }
                                    }

                                    match trpl::run(write_album_art(album_art, &config_values.catbox_user_hash)) {
                                        Ok(filename_link_pair) => {
                                            active_file_image_link = Some(filename_link_pair.1.clone());
                                            filename_hash.insert(filename_link_pair.0, filename_link_pair.1);
                                            write_to_hash_file(&filename_hash);
                                        },
                                        Err(image_error) => {
                                            error_log::log_error("main: write_album_art Error", format!("Error while processing album art image on file {}: {}", &active_file_path, image_error.to_string()).as_str());
                                        }
                                    }
                                }
                                
                            },
                            // Filename does not exist in hash map.
                            None => {
                                // Clear current rich presence information so not visible while uploading.
                                match discord_client.clear_activity() {
                                    Ok(_) => (),
                                    Err(e) => {
                                        error_log::log_error("main: Discord Error on album art update", e.to_string().as_str());
                                    }
                                }

                                match trpl::run(write_album_art(album_art, &config_values.catbox_user_hash)) {
                                    Ok(filename_link_pair) => {
                                        active_file_image_link = Some(filename_link_pair.1.clone());
                                        filename_hash.insert(filename_link_pair.0, filename_link_pair.1);
                                        write_to_hash_file(&filename_hash);
                                    },
                                    Err(image_error) => {
                                        error_log::log_error("main: write_album_art Error", format!("Error while processing album art image on file {}: {}", &active_file_path, image_error.to_string()).as_str());
                                    }
                                }
                            }
                        }
                    }
                    None => active_file_image_link = None,
                }
            } else {
                active_file_image_link = None;
            }

            let album_name_defined = metadata_pack.album.is_some();
            let image_link_defined = active_file_image_link.is_some();

            if album_name_defined && image_link_defined {
                // Both the album and image link are defined. Apply both to Activity.
                match discord_client.set_activity(|a| a._type(ActivityType::Listening)
                                                                .state(&metadata_pack.artist)
                                                                .details(&metadata_pack.title)
                                                                .timestamps(|_t| ActivityTimestamps { start: (Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs())), end: (Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + active_position_duration.1.unwrap())) })
                                                                .assets(|a| {a.large_image(&active_file_image_link.clone().unwrap())
                                                                .large_text(metadata_pack.album.unwrap())}) ) {
                    Ok(_) => (),
                    Err(e) => {
                        error_log::log_error("main: Discord Error on set_activity", e.to_string().as_str());
                    }
                }
            } else if album_name_defined && !image_link_defined {
                // Album is defined, but image link is None. Use default album image, but still apply album name.
                match discord_client.set_activity(|a| a._type(ActivityType::Listening)
                                                                .state(&metadata_pack.artist)
                                                                .details(&metadata_pack.title)
                                                                .timestamps(|_t| ActivityTimestamps { start: (Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs())), end: (Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + active_position_duration.1.unwrap())) })
                                                                .assets(|a| {a.large_image("no_album_art")
                                                                .large_text(metadata_pack.album.unwrap())}) ) {
                    Ok(_) => (),
                    Err(e) => {
                        error_log::log_error("main: Discord Error on set_activity", e.to_string().as_str());
                    }
                }
            } else if !album_name_defined && image_link_defined {
                // Image link is defined, but album name is None. Apply provided image link, but no album name.
                match discord_client.set_activity(|a| a._type(ActivityType::Listening)
                                                                .state(&metadata_pack.artist)
                                                                .details(&metadata_pack.title)
                                                                .timestamps(|_t| ActivityTimestamps { start: (Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs())), end: (Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + active_position_duration.1.unwrap())) })
                                                                .assets(|a| a.large_image(&active_file_image_link.clone().unwrap()))) {
                    Ok(_) => (),
                    Err(e) => {
                        error_log::log_error("main: Discord Error on set_activity", e.to_string().as_str());
                    }
                }
            } else {
                // Both album and image link are None. Use defauly album image, do not provide album name.
                match discord_client.set_activity(|a| a._type(ActivityType::Listening)
                                                                .state(&metadata_pack.artist)
                                                                .details(&metadata_pack.title)
                                                                .timestamps(|_t| ActivityTimestamps { start: (Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs())), end: (Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + active_position_duration.1.unwrap())) })
                                                                .assets(|a| a.large_image("no_album_art"))) {
                    Ok(_) => (),
                    Err(e) => {
                        error_log::log_error("main: Discord Error on set_activity", e.to_string().as_str());
                    }
                }
            }
        }
        
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
    let _ = discord_client.shutdown();
}

fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
    // Attempt to locate home directory and specify config directory.
    let config_dir_path: String = match env::home_dir() {
        Some(path) => path.to_str().unwrap().to_owned() + "/.config/lamp-drpc",
        None => {
            eprintln!("main:load_config:home_dir Error: Could not find home directory.");
            process::exit(1);
        }
    };

    // Determine if config directory exists and is a directory.
    match fs::exists(&config_dir_path) {
        // Config directory exists and is a directory, do nothing.
        Ok(true) if Path::new(&config_dir_path.as_str()).is_dir() => (),
        Ok(true) => { 
            // File exists at config directory path, but is not a directory.
            eprintln!("main:load_config:exists(&config_dir_path) => Ok(true) Error: File at config directory path \"{}\" is not a directory.", config_dir_path);
            process::exit(1);
        },
        Ok(false) => {
            // Config directory does not exist, create it now.
            match fs::create_dir_all(&config_dir_path) {
                Ok(_) => {},
                Err(e) =>  {
                    eprintln!("main:load_config:exists(&config_dir_path):create_dir_all(&config_dir_path) Error: {}", e);
                    process::exit(1);
                },
            }
        },
        Err(e) => { 
            eprintln!("main:load_config:exists(&config_dir_path) Error: {}", e); 
            process::exit(1); 
        }
    }
    
    // Check for configuration file. If it exists, read it. Otherwise, create with default values.
    let config_file_path = config_dir_path + "/lamp.toml";
    match fs::exists(&config_file_path) {
        Ok(true) => {
            // Config file exists, read in values.
            let toml_string = fs::read_to_string(config_file_path)?;
            match toml::from_str(toml_string.as_str()) {
                Ok(config_values) => return Ok(config_values),
                Err(e) => {
                    return Err(Box::from(e));
                }
            }
        },
        Ok(false) => {
            // Configuration file does not exist, create it now and write default values to it.
            let mut config_file = fs::OpenOptions::new()
                                .read(false)
                                .write(true)
                                .create(true)
                                .open(config_file_path)?;
            
            /* 
                Set default configuration values.
                - player_name is the name of the process to be tracked while running. Default is 'cmus'.
                - player_check_delay becomes the amount of time to sleep before checking for the player running.
                  Default is 5.
                - run_secondary_checks determines whether or not player-specific secondary verification of status
                  should be performed. Default is true.
                - va_album_individual indidcates whether or not tracks with "Various Artists" as the album artist
                  should have their album fields blank and album art processed individually. Default is true.
                - catbox_user_hash is used to upload images to the image host, catbox.moe.
            */ 
            write!(config_file,"player_name = \'cmus\'\n\
                                player_check_delay = 5\n\
                                run_secondary_checks = true\n\
                                va_album_individual = true\n")?;

            let config_values = Config {
                player_name: String::from("cmus"),
                player_check_delay: 5,
                run_secondary_checks: true,
                va_album_individual: true,
                catbox_user_hash: None,
            };

            return Ok(config_values);
        },
        Err(e) => { 
            return Err(Box::from(e));
        }
    }
}

fn update_config(new_access_token: String, new_refresh_token: String) -> Result<(), Box<dyn std::error::Error>> {
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
            // File already exists, first read values from it.
            let toml_string = fs::read_to_string(&config_file_path)?;
            let current_config_values: Config = toml::from_str(toml_string.as_str())?;

            // Open config file and overwrite contents, using new tokens.
            let mut config_file = fs::OpenOptions::new()
                                .read(false)
                                .write(true)
                                .truncate(true)
                                .open(&config_file_path)?;
        }
        Ok(false) => {
            // Configuration file does not exist, create it now and write default values to it.
            let mut config_file = fs::OpenOptions::new()
                                .read(false)
                                .write(true)
                                .create(true)
                                .open(config_file_path)?;
            
            let _ = write!(config_file, "player_name = \'cmus\'\n\
                                         player_check_delay = 5\n\
                                         run_secondary_checks = true\n\
                                         va_album_individual = true\n");

            return Err(Box::from("Config file confirmed to not exist during token update. Default configuration created, Imgur information must be added manually."));
        }
        Err(e) => { 
            error_log::log_error("Config Error", &e.to_string().as_str());
            process::exit(1); 
        },
    }

    Ok(())
}

async fn get_link_status(http_client: &reqwest::Client, image_link: &String) -> Result<bool, Box<dyn std::error::Error>> {
    let response = http_client
        .head(image_link)
        .send()
        .await?;

    if response.status() == reqwest::StatusCode::OK { Ok(true) } else { Ok(false) }
}

fn load_hash_file() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    // Check for hashed link file. If it exists, read it, otherwise create blank one.
    let config_dir_path: String; 
    match env::home_dir() {
        Some(path) => {
            config_dir_path = path.to_str().unwrap().to_owned() + "/.config/lamp-drpc";
        },
        None => {
            eprintln!("Error: Could not find home directory.");
            process::exit(1);
        },
    }

    let hash_file_path = config_dir_path + "/link_hash.json";
    let mut filename_hash = HashMap::<String, String>::new();

    match fs::exists(&hash_file_path) {
        Ok(true) => {
            // Read existing hash file.
            let hash_file = File::open(hash_file_path)?;

            let hash_reader = BufReader::new(hash_file);
            filename_hash = serde_json::from_reader(hash_reader)?;

        },
        Ok(false) => {
            // Create new hash file.
            let mut hash_file = fs::OpenOptions::new()
                                        .read(false)
                                        .write(true)
                                        .create(true)
                                        .open(&hash_file_path)?;

            write!(hash_file, "{{\n}}")?;
        }
        Err(e) => {
            return Err(Box::from(e));
        }
    }

    Ok(filename_hash)
}

fn write_to_hash_file(filename_hash: &HashMap<String, String>) -> Result<(), Box<dyn std::error::Error>> {
    // Check for hashed link file. If it exists, read it, otherwise create blank one.
    let config_dir_path: String; 
    match env::home_dir() {
        Some(path) => {
            config_dir_path = path.to_str().unwrap().to_owned() + "/.config/lamp-drpc";
        },
        None => {
            eprintln!("Error: Could not find home directory.");
            process::exit(1);
        },
    }

    let hash_file_path = config_dir_path + "/link_hash.json";
    match fs::exists(&hash_file_path) {
        Ok(true) | Ok(false) => {
            // Write to hash file. If it does not exist, create it again and write to it.
            let mut hash_file = fs::OpenOptions::new()
                                        .read(false)
                                        .write(true)
                                        .truncate(true)
                                        .create(true)
                                        .open(&hash_file_path)?;
            
            let hash_string = serde_json::to_string_pretty(&filename_hash)?;
            write!(hash_file, "{}", hash_string)?;
        },
        Err(e) => {
            return Err(Box::from(e));
        }
    }

    Ok(())
}

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

async fn write_album_art(album_art: AlbumArt, catbox_user_hash: &Option<String>) -> Result<(String, String), Box<dyn std::error::Error>> {
    let mut reader: ImageReader<Cursor<Vec<u8>>>;
    let img: DynamicImage;
    let mime_type = album_art.filename.rsplit_once('.').unwrap().1;
    match mime_type {
        "jpg" | "jpeg" => {
            reader = ImageReader::new(Cursor::new(album_art.data));
            reader.set_format(ImageFormat::Jpeg);
        }
        "png" => {
            reader = ImageReader::new(Cursor::new(album_art.data));
            reader.set_format(ImageFormat::Png);
        }
        &_ => {
            reader = ImageReader::new(Cursor::new(album_art.data)).with_guessed_format()?;
        }
    } 

    // Decode image and get dimensions.
    img = reader.decode()?;
    let dimensions = (img.width(), img.height());

    // Determine new image dimensions based on current dimensions. 
    // If the image is already a square between 512x512 and 1024x1024, no resizing or cropping is necessary.
    let (dst_width, dst_height): (u32, u32);
    let mut dst_image: Image<'_>;
    // Check if image is already square (equal dimensions).
    if dimensions.0 == dimensions.1 {
        if dimensions.0 < 512 {
            (dst_width, dst_height) = (512, 512);
        } else if dimensions.0 > 1024 {
            (dst_width, dst_height) = (1024, 1024);
        } else {
            (dst_width, dst_height) = (dimensions.0, dimensions.1);
        }

        match img.pixel_type() {
            Some(pt) => {
                dst_image = Image::new(dst_width, dst_height, pt);
            }
            None => {
                return Err(Box::from("Pixel type of image could not be determined."));
            }
        }

        // Resize image with no cropping.
        Resizer::new().resize(&img, &mut dst_image, None)?;
    } else {
        let smaller_dimension: u32;
        // Determine which dimension is larger.
        if dimensions.0 > dimensions.1 {
            smaller_dimension = dimensions.1;
        } else {
            smaller_dimension = dimensions.0;
        }

        // Smaller dimension is between 512 and 1024.
        // Set both dimensions to the smaller value.
        if 512 < smaller_dimension && smaller_dimension < 1024 {
            (dst_width, dst_height) = (smaller_dimension, smaller_dimension);
        }
        // Smaller dimension is greater than 1024.
        // Set both dimensions to 1024.
        else if 1024 < smaller_dimension {
            (dst_width, dst_height) = (1024, 1024);
        }
        // Smaller dimension is less than 512.
        // Set both dimensions to 512.
        else {
            (dst_width, dst_height) = (512, 512);
        }

        match img.pixel_type() {
            Some(pt) => {
                dst_image = Image::new(dst_width, dst_height, pt);
            }
            None => {
                return Err(Box::from("Pixel type of image could not be determined."));
            }
        }

        // Resize image with cropping.
        Resizer::new().resize(&img, &mut dst_image, &ResizeOptions::new().fit_into_destination(Some((0.5,0.5))),)?;
    }

    let tempfile_path = format!("{}/{}.{}", env::temp_dir().to_string_lossy(), album_art.filename.rsplit_once('.').unwrap().0, mime_type);
    let tempfile = File::create(&tempfile_path)?;
    let mut result_buf = BufWriter::new(tempfile);

    // Decide on image encoder to use based on mime type.
    match mime_type {
        "jpg" | "jpeg" => JpegEncoder::new(&mut result_buf)
            .write_image(
            dst_image.buffer(),
                dst_width,
                dst_height,
    img.color().into(),)?,
        "png" => PngEncoder::new(&mut result_buf)
            .write_image(
            dst_image.buffer(),
                dst_width,
                dst_height,
    img.color().into(),)?,
        _ => return Err(Box::from("Pixel type of image could not be determined.")),
    }
    
    result_buf.flush()?;

    // Upload file to image host if enough credits are available.
    let uploaded_link = upload_image(&tempfile_path, catbox_user_hash.clone()).await?;

    // Delete file from tmp.
    remove_file(tempfile_path)?;

    println!("album filename: {}\nimgur link: {}", album_art.filename, uploaded_link);
    Ok((album_art.filename, uploaded_link))
}

async fn upload_image(image_path: &String, catbox_user_hash: Option<String>) -> Result<String, Box<dyn std::error::Error>> {
    let uploaded = from_file(image_path, catbox_user_hash.as_ref()).await?;
    Ok(uploaded)
}