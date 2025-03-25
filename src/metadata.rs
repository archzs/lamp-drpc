use audiotags::components::FlacTag;
use audiotags::{AudioTagEdit, MimeType};
use claxon::{FlacReader, FlacReaderOptions};
use id3::{Content, Tag, TagLike};

use crate::error_log;

pub struct AlbumArt {
    pub filename: String,
    pub data: Vec<u8>,
}

pub struct MetadataPackage {
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub artist: String,
    pub title: String,
    pub album_art: Option<AlbumArt>,
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

pub fn read_metadata(active_file_path: &String, va_album_individual: &bool) -> Option<MetadataPackage> {
    // Determine which tag reader to used based on file extension.
    match active_file_path.rsplit_once('.').unwrap().1 {
        "flac" => return read_vorbis(&active_file_path, &va_album_individual),
        "mp3" | "wav" => return read_id3(&active_file_path, &va_album_individual),
        _ => {
            error_log::log_error("metadata:read_metadata Error", format!("The file at {} is not in a supported format.", active_file_path).as_str());
            return None;
        }
    }
}

fn hash_filename(album_artist: &Option<String>, album: &Option<String>, year: Option<String>, mime_type: &str, image_data: &Vec<u8>) -> String {
    // Construct (probably) album-unique string to be hashed as first half of filename.
    let metadata_string = album_artist.clone().unwrap_or(String::from("0")) 
                                + album.clone().unwrap_or(String::from("0")).as_str()
                                + year.clone().unwrap_or(String::from("0")).as_str();

    // Hash metadata string and image bytes for first and second halves of filename, then concat image extension.
    let hashed_filename = format!("{}-{}{}", CRC32.checksum(metadata_string.as_bytes()).to_string(), CRC32.checksum(image_data).to_string(), mime_type);
    println!("{}", hashed_filename);
    return hashed_filename;
}

fn read_vorbis(active_file_path: &String, va_album_individual: &bool) -> Option<MetadataPackage> {
    match FlacReader::open_ext(&active_file_path, FlacReaderOptions { metadata_only: true, read_vorbis_comment: true }) {
        Ok(vorbis_tag) => {
            let mut metadata_pack = MetadataPackage::default();

            // Declare variables for relevant tags.
            let mut album_tag: Option<String> = None;
            let mut album_artist_vec = Vec::<String>::new();
            let mut artist_vec = Vec::<String>::new();
            let mut title_tag: Option<String> = None;
            let mut year_tag: Option<String> = None;

            // Get all tags and iterate through them.
            for tag in vorbis_tag.tags() {
                match tag.0 {
                    "album" => album_tag = Some(tag.1.to_owned()),
                    "albumartist" => album_artist_vec.push(tag.1.to_owned()),
                    "artist" => artist_vec.push(tag.1.to_owned()),
                    "title" => title_tag = Some(tag.1.to_owned()),
                    "year" => year_tag = Some(tag.1.to_owned()),
                    &_ => (),
                }
            }

            // Assign metadata values based on retrieved tags.
            // album
            if album_tag.is_some() {
                metadata_pack.album = album_tag;
            } else {
                metadata_pack.album = None;
            }

            // albumartist
            if album_artist_vec.len() > 0 {
                metadata_pack.album_artist = Some(album_artist_vec.join(", "));

                // If va_album_individual is enabled, album_artist is "Various Artists", and the album is "Various Artists", album tag is not kept.
                if *va_album_individual && metadata_pack.album_artist == Some(String::from("Various Artists")) 
                                        && metadata_pack.album == Some(String::from("Various Artists")) {
                    metadata_pack.album = None;
                }
            } else {
                metadata_pack.album_artist = None; 
            } 

            // artist (Tag is required for basic functionality, so return None if not present)
            if artist_vec.len() > 0 {
                metadata_pack.artist = artist_vec.join(", ");
            } else {
                error_log::log_error("metadata:read_vorbis Warning", format!("No artist tag(s) were found in file {}.", active_file_path).as_str());
                return None;
            }

            // title (Tag is required for basic functionality, so return None if not present)
            if let Some(title) = title_tag {
                metadata_pack.title = title;
            } else {
                error_log::log_error("metadata:read_vorbis Warning", format!("No title tag(s) were found in file {}.", active_file_path).as_str());
                return None;
            }

            // year (Used only for constructing filename hash, not included in metadata package.)
            let album_year: Option<String> = year_tag;

            // album_art
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
                                    error_log::log_error("metadata:read_vorbis:album_art.mime_type match Error", format!("Album cover in file {} is of unsupported mime type {:?}.", &active_file_path, _a).as_str());
                                    metadata_pack.album_art = None;
                                }
                            }
                        }
                        // File does not have album art tagged.
                        None => metadata_pack.album_art = None,
                    }
                }
                Err(e) => {
                    error_log::log_error("metadata:read_vorbis:FlacTag::read_from_path() match Error", format!("Album art could not be extracted from the file at {}:\n{:?}", active_file_path, e).as_str());
                    metadata_pack.album_art = None;
                }
            }

            Some(metadata_pack)
        }
        Err(e) => {
            error_log::log_error("metadata:read_vorbis:open_ext() match Error", format!("Vorbis comments could not be read from the file at {}:\n{:?}", active_file_path, e).as_str());
            return None;
        }
    }
}

fn read_id3(active_file_path: &String, va_album_individual: &bool) -> Option<MetadataPackage> {
    match Tag::read_from_path(&active_file_path) {
        Ok(id3_tag) => {
            let mut metadata_pack = MetadataPackage::default();

            // Retrieve metadata from from specified file.
            // album_artist
            metadata_pack.album_artist = id3_tag.album_artist().map(|album_artist| album_artist.to_string());
            
            // album
            let album_tag = id3_tag.album().map(|album| album.to_string()).unwrap_or_default();

            // If va_album_individual is enabled, album_artist is "Various Artists", and album is "Various Artists", album tag is not kept.
            if (*va_album_individual && metadata_pack.album_artist == Some(String::from("Various Artists")) 
                                     && album_tag == String::from("Various Artists")) 
                                     || album_tag == String::default() {
                metadata_pack.album = None;
            } else {
                metadata_pack.album = Some(album_tag);
            }
            
            // artist (Tag is required for basic functionality, so return None if not present)
            match id3_tag.artists() {
                Some(artists) => metadata_pack.artist = artists.join(", "),
                None => {
                    error_log::log_error("metadata:read_id3:id3_tag.artists() match Warning", format!("No artist tag(s) were found in file {}.", active_file_path).as_str());
                    return None;
                }
            }

            // title (Tag is required for basic functionality, so return None if not present)
            match id3_tag.title() {
                Some(title) => metadata_pack.title = title.to_owned(),
                None => {
                    error_log::log_error("metadata:read_id3:id3_tag.title() match Warning", format!("No artist tag(s) were found in file {}.", active_file_path).as_str());
                    return None;
                }
            }

            // year
            // Used only for constructing filename hash, not included in metadata package.
            let album_year: Option<String> = id3_tag.year().map(|year| year.to_string());
            
            // album_art
            let extracted_images = id3_tag.pictures().collect::<Vec<_>>();
            if extracted_images.len() > 0 {
                match Content::Picture(extracted_images[0].clone()).picture() {
                    Some(album_art) => {
                        match album_art.mime_type.as_str() {
                            "image/jpeg" => metadata_pack.album_art = Some(AlbumArt { filename: hash_filename(&metadata_pack.album_artist, &metadata_pack.album, album_year, ".jpg", &album_art.data), data: album_art.data.clone() }),
                            "image/png"  => metadata_pack.album_art = Some(AlbumArt { filename: hash_filename(&metadata_pack.album_artist, &metadata_pack.album, album_year, ".png", &album_art.data), data: album_art.data.clone() }),
                            _ => metadata_pack.album_art = None,
                        }
                    }
                    None => metadata_pack.album_art = None,
                }
            } else {
                metadata_pack.album_art = None;
            }

            return Some(metadata_pack);
        }
        Err(e) => {
            error_log::log_error("metadata:read_id3:Tag::read_from_path() match Error", format!("ID3 tags could not be read from the file at {}:\n{}", &active_file_path, e).as_str());
            return None;
        }    
    }
}