use std::env;
use std::collections::HashMap;
use std::fs::{remove_file, File};
use std::io::{BufReader, Cursor};
use std::thread;
use std::time::Duration;
use serde::Deserialize;
use serde_json::{Deserializer, Serializer, Value};
use sysinfo::{Pid, ProcessStatus, ProcessesToUpdate, ProcessRefreshKind, RefreshKind, System};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{DynamicImage, ImageEncoder, ImageFormat, ImageReader};
use std::io::BufWriter;
use fast_image_resize::images::Image;
use fast_image_resize::{IntoImageView, Resizer, ResizeOptions};
use http::{Method, StatusCode};
use imgurs::{send_api_request, ImgurClient};

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

    fn get_position_and_duration(&self) -> (Option<u32>, Option<u32>) {
        match self {
            MusicPlayer::Cmus(cmus) => return Cmus::get_position_and_duration(&cmus),
        }
    }
}

#[derive(Clone, Deserialize)]
struct ImgurInfo {
    clientId: String,
    clientSecret: String,
    album_id: Option<String>,
    access_token: Option<String>,
    refresh_token: Option<String>,
}

impl ImgurInfo {
    fn update_tokens(&mut self, new_access_token: String, new_refresh_token: String) {
        self.access_token = Some(new_access_token.clone());
        self.refresh_token = Some(new_refresh_token.clone());

        // Update lamp.toml with new tokens.
        match update_config(new_access_token, new_refresh_token) {
            Ok(()) => (),
            Err(e) => {
                error_log::log_error("Config Update Error", e.to_string().as_str());
                process::exit(1);
            }
        }
    }
}

#[derive(Deserialize)]
struct Config {
    player_name: String,
    player_check_delay: u64,
    run_secondary_checks: bool,
    va_album_individual: bool,
    imgur: Option<ImgurInfo>,
}

fn main() {
    let mut config_values: Config = load_config();
    let mut filename_hash = match load_hash_file() {
        Ok(filename_hash) => filename_hash,
        Err(e) => {
            error_log::log_error("Hash File Error", e.to_string().as_str());
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
    let mut active_file_image_link= String::new(); // Link to the album art of the currently playing track, hosted on Imgur.
    let mut active_position_duration: (Option<u32>, Option<u32>) = (None, None); // The position of current playback and duration of audio file.
    
    // Begin main loop
    while player_status != ProcessStatus::Stop {
        // Update active file path, duration, and position
        active_file_path = active_music_player.get_active_file_path();
        active_position_duration = active_music_player.get_position_and_duration();

        // If active file has changed, then read metadata of new file.
        if active_file_path != previous_file_path {
            // Create and fill new MetadataPackage.
            let metadata_pack = read_metadata(&active_file_path, &config_values.va_album_individual).unwrap();

            // Check if Imgur information is defined in config file.
            // clientId known to be defined, since an error would have already been thrown otherwise.
            if config_values.imgur.is_some() {
                match metadata_pack.album_art {
                    Some(album_art) => {
                        if filename_hash.contains_key(&album_art.filename) {
                            // Test page is not 404
                        } else {
                            // Filename not already in hash map, so album art may not be in image host.
                            // Write the image to a temporary file, upload it to image host, and add to hash map.

                            // Clear current rich presence information so not visible while uploading.
                            //todo!();

                            match trpl::run(write_album_art(album_art, config_values.imgur.clone().unwrap())) {
                                Ok(filename_link_pair) => {
                                    filename_hash.insert(filename_link_pair.0, filename_link_pair.1);
                                    write_to_hash_file(&filename_hash);
                                },
                                Err(image_error) => {
                                    error_log::log_error("Image Error", format!("Error while processing album art image on file {}: {}", &active_file_path, image_error.to_string()).as_str());
                                }
                            }
                        }
                    }
                    None => (),
                }
            }
            thread::sleep(rest_time);
            
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
                - va_album_individual indidcates whether or not tracks with "Various Artists" as the album artist
                  should have their album fields blank and album art processed individually. Default is true.
                [imgur] contains information used for uploading images to Imgur.
                - clientId is used for making API calls with the imgurs crate functions.
                - clientSecret is used in select functions, like refreshing access tokens.
                - album_id is optionally used for moving uploaded images to a specified album. (only with auth)
                - refresh_token is used to obtain a new access token when it expires.
                - access_token is used for authenticating when performing actions as a certain user,

                Since it is optional, the following must be added to the config file to use Imgur functionality:
                [imgur]
                clientId = ''
            */ 
            match config_file {
                Ok(_) =>  {
                    let _ = write!(config_file.expect("Configuration file should exist and be accessible at this point."), "player_name = \'cmus\'\n\
                                                                                                                            player_check_delay = 5\n\
                                                                                                                            run_secondary_checks = true\n\
                                                                                                                            va_album_individual = true\n");
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
                va_album_individual: true,
                imgur: None,
            };
            return config_values;
        },
        Err(e) => { 
            error_log::log_error("Config Error", &e.to_string().as_str());
            process::exit(1); 
        },
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

            match current_config_values.imgur {
                Some(current_imgur_info) => {
                    if current_imgur_info.album_id.is_some() {
                        let _ = write!(config_file, "player_name = \'{}\'\n\
                                                     player_check_delay = {}\n\
                                                     run_secondary_checks = {}\n\
                                                     va_album_individual = {}\n\n\
                                                     [imgur]\n\
                                                     clientId = \'{}\'\n\
                                                     clientSecret = \'{}\'\n\
                                                     album_id = \'{}\'\n\
                                                     access_token = \'{}\'\n\
                                                     refresh_token = \'{}\'", 
                            current_config_values.player_name, current_config_values.player_check_delay, current_config_values.run_secondary_checks, current_config_values.va_album_individual,
                            current_imgur_info.clientId, current_imgur_info.clientSecret, current_imgur_info.album_id.unwrap(), new_access_token, new_refresh_token);
                    } else {
                        let _ = write!(config_file, "player_name = \'{}\'\n\
                                                     player_check_delay = {}\n\
                                                     run_secondary_checks = {}\n\
                                                     va_album_individual = {}\n\n\
                                                     [imgur]\n\
                                                     clientId = \'{}\'\n\
                                                     clientSecret = \'{}\'\n\
                                                     access_token = \'{}\'\n\
                                                     refresh_token = \'{}\'", 
                            current_config_values.player_name, current_config_values.player_check_delay, current_config_values.run_secondary_checks, current_config_values.va_album_individual,
                            current_imgur_info.clientId, current_imgur_info.clientSecret, new_access_token, new_refresh_token);
                    }
                },
                None => {
                    return Err(Box::from("Imgur information section has been removed from config file during token update."));
                },
            }
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

async fn write_album_art(album_art: AlbumArt, mut imgur_info: ImgurInfo) -> Result<(String, String), Box<dyn std::error::Error>> {
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
    let imgur_client = ImgurClient::new(&imgur_info.clientId);
    let uploaded_link = upload_image(imgur_client, &mut imgur_info, &tempfile_path).await?;

    // Delete file from tmp.
    remove_file(tempfile_path)?;

    println!("album filename: {}\nimgur link: {}", album_art.filename, uploaded_link);
    Ok((album_art.filename, uploaded_link))
}

async fn upload_image(imgur_client: ImgurClient, imgur_info: &mut ImgurInfo, image_path: &String) -> Result<String, Box<dyn std::error::Error>> {
    let rate_limit = imgur_client.rate_limit().await?;
    let uploaded: imgurs::ImageInfo;

    if rate_limit.data.client_remaining >= 10 {
        uploaded = imgur_client.upload_image(&image_path).await?;
    } else {
        return Err(Box::from("Imgur clientId does not have enough credits remaining to upload an image."));
    }

    /* // Check if access_token and refresh_token are defined.
    // If both are defined and accurate, moving into an album is supported.
    if imgur_info.access_token.clone().is_some() && imgur_info.refresh_token.clone().is_some() {
        // If album_id is defined, attempt to move new image into album provided.
        match imgur_info.album_id.clone() {
            Some(album_id) => {
                // Check for 10 + 3 credits.
                if rate_limit.data.client_remaining >= 13 {
                    // Upload image, record info.
                    uploaded = imgur_client.upload_image(&image_path).await?;
                    println!("Upload success");  

                    // Attempt to move uploaded image to album with provided album_id.
                    let mut api_hash: HashMap<&str, String> = HashMap::from([("ids[]", uploaded.data.id.clone()),]);
                    let mut http_response = send_api_request(&imgur_client, Method::POST, format!("https://api.imgur.com/3/album/{}/add", album_id), Some(api_hash)).await?;
                    println!("status: {:?}", http_response.headers());
                    // If 401 is returned, access_token may have expired..
                    if http_response.status() == StatusCode::from_u16(401).unwrap() {
                        println!("401");
                        // Construct and send API request for new access_token.
                        api_hash = HashMap::from([
                            ("refresh_token", imgur_info.refresh_token.clone().unwrap()),
                            ("clientId", imgur_info.clientId.clone()),
                            ("clientSecret", imgur_info.clientSecret.clone()),
                            ("grant_type", "refresh_token".to_owned()),
                        ]);
                        http_response = send_api_request(&imgur_client, Method::POST, "https://api.imgur.com/oauth2/token".to_owned(), Some(api_hash)).await?;
                        let response_body = http_response.text().await?;
                        let body_json: Value = serde_json::from_str(&response_body)?;
                        imgur_info.update_tokens(body_json["access_token"].to_string(), body_json["refresh_token"].to_string());

                        // With new access_token, try previous request again.
                        api_hash = HashMap::from([("ids[]", uploaded.data.id),]);
                        http_response = send_api_request(&imgur_client, Method::POST, format!("https://api.imgur.com/3/album/{}/add", album_id), Some(api_hash)).await?;
                        if http_response.status() != StatusCode::from_u16(200).unwrap() {
                            return Err(Box::from(format!("Imgur API request to add image to album failed after token update with status: {}", http_response.status()).as_str()));
                        }
                        println!("Success move");
                    }
                } else {
                    return Err(Box::from("Imgur clientId does not have enough credits remaining to upload an image and move to provided album."));
                }
            }
            // No album_id provided, just upload image.
            None => {
                if rate_limit.data.client_remaining >= 10 {
                    uploaded = imgur_client.upload_image(&image_path).await?;       
                } else {
                    return Err(Box::from("Imgur clientId does not have enough credits remaining to upload an image."));
                }
            },
        }
    } else {
        // No access_token and refresh_token provided, only upload image.
        if rate_limit.data.client_remaining >= 10 {
            uploaded = imgur_client.upload_image(&image_path).await?;       
        } else {
            return Err(Box::from("Imgur clientId does not have enough credits remaining to upload an image."));
        }
    } */

   Ok(uploaded.data.link)
}