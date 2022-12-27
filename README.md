# tuitch
Tuitch is a Twitch chat TUI that implements Twitch chat's basic functionality into your terminal. You can join Twitch chat channels anonymously or with your own Twitch account using your Twitch OAuth token. The token is saved locally on your machine in a `Config.toml` file.

Tuitch uses the [`twitch_irc`](https://docs.rs/twitch-irc/3.0.1/twitch_irc/) crate to communicate with the Twitch servers and [`termion`](https://docs.rs/termion/1.5.6/termion/) for a light and simple UI. See the `Cargo.toml` file for the full list of dependancies. 

This project was a learning opportunity, and isn't likely to be finished or expanded on.

## Install
Right now, I haven't built any deployment or installation for the project, so you'll need to clone the repository yourself. This project is in early development and I only have so much free time on my hands.

## Use Tuitch
Tuitch comes with very basic commands and functionality. A list of commands is shown on the home page when the appliction starts, they include `:join <channel>` to join a Twitch channel's chatroom and `:credentials <username> <oauth token>` to update your config file's Twitch user credentials.
