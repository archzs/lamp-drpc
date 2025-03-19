use audiotags::components::FlacTag;
use audiotags::{AudioTagEdit, MimeType};
use claxon::FlacReader;
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