# lamp-drpc<br> Local Audio/Music Player - Discord Rich Presence

Lamp is a tool for UNIX-based systems providing Discord's rich presence with information from local audio players, such as cmus. It is designed to be somewhat easily extended to support any local music player possessing functionality allowing identification of the currently playing file.

## Features
- Reads and displays album, artist, and title (plus albumartist and year) from files in ID3 and Vorbis Comment tag formats.
- Reads and automatically uploads embedded album art to catbox.moe to display images via URL. JPG and PNG formats are currently supported. (Requires catbox.moe account)

## Screenshots
![image](https://github.com/user-attachments/assets/b86deabf-48a2-4dc9-9f5e-02339e36a3e5) <br>
![image](https://github.com/user-attachments/assets/85ed4310-c0bd-4c38-8c1a-d905426234bb) ![image](https://github.com/user-attachments/assets/519341c7-48e4-406e-895b-8aaba6c81033)

## Configuration

A default configuration file is created under ~/.config/lamp-drpc upon starting Lamp, if one does not already exist. <br>

<code>player_name</code>: Name of the intended music player's process. Used to find the player's PID at startup. <br>
<code>player_check_delay</code>: Number of seconds to wait before finding player PID. Intended to allow time for music player to initialize. <br>
<code>run_secondary_checks</code>: Enables/Disables secondary assurance(s) that player is still running (beyond checking for the PID). <br>
<code>va_album_individual</code>: Enables/Disables display of album name on rich presence if both album and albumartist fields are "Various Artists". <br>
<code>catbox_user_hash</code>: User hash used for uploaded images to catbox.moe. As this is optional and requires user input, it is not included in the default configuration file and must be manually added. <br>

Code to support any other music players with the ability to identify actively playing tracks should be added as exemplified at the locations indicated with "[PLAYER IMPLEMENTATION HERE]".
