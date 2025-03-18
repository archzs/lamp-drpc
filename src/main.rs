use std::convert::TryFrom;
use std::env;
use std::fs::File;
use std::io::Cursor;
use std::thread;
use std::time::Duration;
use audiotags::components::FlacTag;
use audiotags::{Album, AudioTagEdit, MimeType};
use claxon::FlacReader;
use id3::{Content, Tag, TagLike};
use serde::Deserialize;
use sysinfo::{Pid, ProcessStatus, ProcessesToUpdate, ProcessRefreshKind, RefreshKind, System};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{ImageEncoder, ImageReader};
use std::io::BufWriter;
use fast_image_resize::images::Image;
use fast_image_resize::{IntoImageView, Resizer, ResizeOptions};

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

#[derive(Deserialize)]
struct Config {
    player_name: String,
    player_check_delay: u64,
    run_secondary_checks: bool,
    va_album_individual: bool,
}

struct AlbumArt {
    filename: String,
    data: Vec<u8>,
}

struct MetadataPackage {
    album_artist: Option<String>,
    album: Option<String>,
    artist: String,
    title: String,
    album_art: Option<AlbumArt>,
}

impl Default for MetadataPackage {
    fn default() -> Self {
        MetadataPackage {
            album_artist: None,
            album: None,
            artist: String::new(),
            title: String::new(),
            album_art: None,
        }
    }
}

// Global CRC32 hasher for album art filename hashing
const CRC32: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_CKSUM);

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
            println!("album_artist: {}\nalbum: {}\nartist: {}\ntitle: {}", metadata_pack.album_artist.unwrap(), metadata_pack.album.unwrap(), metadata_pack.artist, metadata_pack.title);
            println!("Album art filename: {}", metadata_pack.album_art.unwrap().filename);
            //fs::write("/home/zera/workspace/test.jpg", metadata_pack.album_art.unwrap().data).unwrap();
            /* let mut reader = ImageReader::new(Cursor::new(metadata_pack.album_art.unwrap().data))
                .with_guessed_format()
                .expect("Cursor io never fails");
            //let mut reader = ImageReader::open("/home/zera/workspace/folder.jpg").unwrap();
            let img = reader.decode().unwrap();
            let sizes = (img.width(), img.height());
            println!("Dimensions: {}, {}", sizes.0, sizes.1);

            
            let dst_width: u32;
            let dst_height: u32;
            if sizes.0 < 512 || sizes.1 < 512 {
                //dst_image = Image::new(512, 512, img.pixel_type().unwrap(),);
                dst_width = 512;
                dst_height = 512;
            } else {
                //dst_image = Image::new(sizes.0, sizes.1, img.pixel_type().unwrap(),);
                dst_width = 1024;
                dst_height = 1024;
            }
            let mut dst_image = Image::new(dst_width, dst_height, img.pixel_type().unwrap(),);

            let mut resizer = Resizer::new();
            resizer.resize(&img, &mut dst_image, &ResizeOptions::new().fit_into_destination(Some((0.5,0.5))),).unwrap();

            let mut result_buf = BufWriter::new(File::create("/home/zera/workspace/test.jpg").unwrap());
            JpegEncoder::new(&mut result_buf)
                .write_image(
                    dst_image.buffer(),
                    dst_width,
                    dst_height,
                    img.color().into(),
                )
                .unwrap();
            //fs::write("/home/zera/workspace/test.jpg", result_buf.buffer()).unwrap(); */
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
            };
            return config_values;
        },
        Err(e) => { 
            error_log::log_error("Error", &e.to_string().as_str());
            process::exit(1); 
        },
    }
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

fn read_metadata(active_file_path: &String, va_album_individual: &bool) -> Option<MetadataPackage> {
    // Determine which tag reader to used based on file extension.
    //FIX MATCH
    match active_file_path.rsplit_once('.').unwrap().1 {
        "flac" => return read_vorbis(&active_file_path, &va_album_individual),
        "mp3" | "wav" => return read_id3(&active_file_path, &va_album_individual),
        _ => {
            error_log::log_error("File Error", format!("The file at {} is not in a supported format.", active_file_path).as_str());
            return None;
        }
    }
}

fn hash_filename(album_artist: &Option<String>, album: &Option<String>, year: Option<u16>, mime_type: &str, image_data: &Vec<u8>) -> String {
    // Construct (probably) album-unique string to be hashed as first half of filename.
    let mut metadata_string = String::new();
    // Concat album_artist if Some.
    metadata_string.push_str(album_artist.clone().unwrap_or(String::from("0")).as_str());
    // Concat album if Some.
    metadata_string.push_str(album.clone().unwrap_or(String::from("0")).as_str());
    // Concat year if Some.
    metadata_string.push_str(year.unwrap_or(0).to_string().as_str());

    // Hash metadata string and image bytes for first and second halves of filename, then concat image extension.
    let mut hashed_filename = format!("{}-{}", CRC32.checksum(metadata_string.as_bytes()).to_string(), CRC32.checksum(image_data).to_string());
    hashed_filename.push_str(mime_type);
    return hashed_filename;
}

fn read_vorbis(active_file_path: &String, va_album_individual: &bool) -> Option<MetadataPackage> {
    match FlacReader::open(&active_file_path) {
        Ok(vorbis_tag) => {
            let mut metadata_pack = MetadataPackage::default();

            // Retrieve fields from specified file.
            // album_artist
            let mut album_artist_vec = Vec::<String>::new();
            for album_artist in vorbis_tag.get_tag("albumartist") {
                album_artist_vec.push(album_artist.to_owned());
            }

            if album_artist_vec.len() > 0 {
                metadata_pack.album_artist = Some(album_artist_vec.join(", "));
            } else {
                metadata_pack.album_artist = None;
            }

            // album
            let mut album_vec = Vec::<&str>::new();
            for album in vorbis_tag.get_tag("album") {
                album_vec.push(album);
            }
            if album_vec.len() > 0 {
                metadata_pack.album = Some(album_vec[0].to_owned());
            } else {
                metadata_pack.album = None;
            }

            // If va_album_individual is enabled, album_artist is "Various Artists", and the album is "Various Artists", album tag is not kept.
            if *va_album_individual && album_artist_vec[0] == String::from("Various Artists") && album_vec[0] == String::from("Various Artists") {
                metadata_pack.album = None;
            }

            // artist (Tag is required for basic functionality, so return None if not present)
            let mut artist_vec = Vec::<String>::new();
            for artist in vorbis_tag.get_tag("artist") {
                artist_vec.push(artist.to_owned());
            }
            if artist_vec.len() > 0 {
                metadata_pack.artist = artist_vec.join(", ");
            } else {
                return None;
            }

            // title (Tag is required for basic functionality, so return None if not present)
            let mut title_vec = Vec::<&str>::new();
            for title in vorbis_tag.get_tag("title") {
                title_vec.push(title);
            }
            if title_vec.len() > 0 {
                metadata_pack.title = title_vec[0].to_owned();
            } else {
                return None;
            }

            // year
            // Used only for constructing filename hash, not included in metadata package.
            let mut year_vec = Vec::<&str>::new();
            let album_year: Option<u16>;
            for year in vorbis_tag.get_tag("year") {
                year_vec.push(year);
            }
            if year_vec.len() > 0 {
                match year_vec[0].parse::<u16>() {
                    Ok(y) => album_year = Some(y),
                    Err(parse_error) => {
                        error_log::log_error("Metadata Error", format!("Year tagged on file {} is not a u16 integer: {}", &active_file_path, parse_error.to_string()).as_str());
                        album_year = None;
                    }
                }
            } else {
                album_year = None;
            }

            // album_art
            /*
                Determine whether or not to extract and upload image here. Based on va_album_individual.
            */
            match FlacTag::read_from_path(&active_file_path) {
                Ok(flac_tag) => {
                    match flac_tag.album_cover() {
                        Some(album_art) => {
                            let new_image: AlbumArt;
                            match album_art.mime_type {
                                MimeType::Jpeg => {
                                    // Hash album art filename
                                    new_image = AlbumArt { filename: hash_filename(&metadata_pack.album_artist, &metadata_pack.album, album_year, ".jpg", &album_art.data.to_vec()), data: album_art.data.to_vec() };
                                    metadata_pack.album_art = Some(new_image);
                                },
                                MimeType::Png =>  {
                                    // Hash album art filename
                                    new_image = AlbumArt { filename: hash_filename(&metadata_pack.album_artist, &metadata_pack.album, album_year, ".png", &album_art.data.to_vec()), data: album_art.data.to_vec() };
                                    metadata_pack.album_art = Some(new_image);
                                },
                                _a => { // For any other types
                                    error_log::log_error("Metadata Error", format!("Album cover in file {} is of unsupported mime type {:?}.", &active_file_path, _a).as_str());
                                    metadata_pack.album_art = None;
                                }
                            }
                        }
                        None => {
                            metadata_pack.album_art = None;
                        }
                    }
                }
                Err(vorbis_error) => {
                    error_log::log_error("Metadata Error", format!("Album art could not be extracted from the file at {}:\n{:?}", active_file_path, vorbis_error).as_str());
                    metadata_pack.album_art = None;
                }
            }

            Some(metadata_pack)
        }
        Err(vorbis_error) => {
            error_log::log_error("Metadata Error", format!("Vorbis comments could not be read from the file at {}:\n{:?}", active_file_path, vorbis_error).as_str());
            return None;
        }
    }
}

fn read_id3(active_file_path: &String, va_album_individual: &bool) -> Option<MetadataPackage> {
    match Tag::read_from_path(&active_file_path) {
        Ok(id3_tag) => {
            let mut metadata_pack = MetadataPackage::default();

            // Retrieve fields from specified file.
            // album_artist
            let mut album_artist_compare = String::new();
            match id3_tag.album_artist().map(|album_artist| album_artist.to_string()) {
                Some(album_artist) =>  {
                    album_artist_compare = album_artist;
                    metadata_pack.album_artist = Some(album_artist_compare.clone())
                },
                None => metadata_pack.album_artist = None,
            }
            
            // album
            // If va_album_individual is enabled, album_artist is "Various Artists", and album is "Various Artists", album tag is not kept.
            let album_tag = id3_tag.album().map(|album| album.to_string()).unwrap_or_default();
            if (*va_album_individual && album_artist_compare == String::from("Various Artists") && album_tag == String::from("Various Artists")) 
                || album_tag == String::default() {
                metadata_pack.album = None;
            } else {
                metadata_pack.album = Some(album_tag);
            }
            
            // artist (Tag is required for basic functionality, so return None if not present)
            match id3_tag.artists() {
                Some(artists) => {
                    metadata_pack.artist = artists.join(", ");
                },
                None => {
                    error_log::log_error("Metadata Error", format!("No artist tag(s) were found in file {}.", active_file_path).as_str());
                    return None;
                },
            }

            // title (Tag is required for basic functionality, so return None if not present)
            match id3_tag.title() {
                Some(title) => metadata_pack.title = title.to_owned(),
                None => {
                    error_log::log_error("Metadata Error", format!("No title tag was found in file {}.", active_file_path).as_str());
                    return None;
                }
            }

            // year
            // Used only for constructing filename hash, not included in metadata package.
            let album_year: Option<u16>;
            match id3_tag.year() {
                Some(year) => {
                    match u16::try_from(year) {
                        Ok(y) => album_year = Some(y),
                        Err(parse_error) => {
                            error_log::log_error("Metadata Error", format!("Year tagged on file {} is not a u16 integer: {}", &active_file_path, parse_error.to_string()).as_str(),);
                            album_year = None;
                        }
                    }
                }
                None => album_year = None,
            };
            
            // album_art
            /*
                Determine whether or not to extract and upload image here. Based on va_album_individual.
            */
            let extracted_images = id3_tag.pictures().collect::<Vec<_>>();
            if extracted_images.len() > 0 {
                match Content::Picture(extracted_images[0].clone()).picture() {
                    Some(album_art) => {
                        let new_image: Option<AlbumArt>;
                        match album_art.mime_type.as_str() {
                            "image/jpeg"=> new_image = Some(AlbumArt { filename: hash_filename(&metadata_pack.album_artist, &metadata_pack.album, album_year, ".jpg", &album_art.data), data: album_art.data.clone() }),
                            "image/png" => new_image = Some(AlbumArt { filename: hash_filename(&metadata_pack.album_artist, &metadata_pack.album, album_year, ".png", &album_art.data), data: album_art.data.clone() }),
                            _ => new_image = None,
                        }
                        metadata_pack.album_art = new_image;
                    }
                    None => metadata_pack.album_art = None,
                }
            } else {
                metadata_pack.album_art = None;
            }

            return Some(metadata_pack);
        }
        Err(id3_error) => {
            error_log::log_error("Metadata Error", format!("ID3 tags could not be read from the file at {}:\n{}", &active_file_path, id3_error).as_str());
            return None;
        }    
    }
}