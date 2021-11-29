use crate::commands::run_command;
use crate::messages::{format_message, print_message, send_user_message};
use crate::user_config::{get_client_config, set_client_config};
use crate::user_interface::home_screen;
use std::{io::stdout, io::Write, sync::{Arc, mpsc::{self, TryRecvError}}};
use termion::{input::TermRead, raw::IntoRawMode, screen::AlternateScreen};
use tokio::{select, sync::broadcast, sync::RwLock, task};
use twitch_irc::{login::StaticLoginCredentials, SecureTCPTransport, TwitchIRCClient};

mod commands;
mod messages;
mod user_config;
mod user_interface;

#[tokio::main]
pub async fn main() -> std::io::Result<()> {
    // TODO: Add another tokio task for ctrl-c handling.
    // TODO: Need error-handling for channels
    // that do not exist and incorrect user input.

    // User config path and the config struct itself,
    // the struct is built from the contents of the config file
    // and used to access the current username data.
    // this is probably a temp setup for config while in development
    // and will likely change when a more streamlined login
    // and config system is done, including a working :login
    // command for the user.
    let config_path = "Config.toml";
    let user_config = get_client_config(config_path).await;

    let current_channel = Arc::new(RwLock::new(String::new()));
    let user_name = Arc::new(RwLock::new(user_config.username));
    let current_channel_read = Arc::clone(&current_channel);
    let _user_name_read = Arc::clone(&user_name);

    // Input-buffer for user's typed input and chat messages.
    // This is a shared state to allow proper handling with incoming
    // server messages while unsent user input is in the console.
    let input_buffer_lock = Arc::new(RwLock::new(String::new()));
    let input_buffer = Arc::clone(&input_buffer_lock);
    let input_buffer2 = Arc::clone(&input_buffer_lock);

    // Create tx/rx to send and receive shutdown signal
    // when specific user input is detected.
    let (shutdown_tx, mut shutdown_rx) = broadcast::channel(2);
    let mut shutdown_rx2 = shutdown_tx.subscribe();

    // Channel for chat-line commands and settings.
    let (command_tx, mut command_rx) = broadcast::channel(2);

    // The TwitchIRCClient is built with either the default (read-only) or Twitch
    // login credentials (username & OAuth token pair).
    let (mut incoming_messages, client) = TwitchIRCClient::<
        SecureTCPTransport,
        StaticLoginCredentials,
    >::new(set_client_config(config_path).await);

    // TwitchIRCClient is thread safe, clone() can be called here.
    // client2 is used to send user messages to the Twitch servers.
    let client2 = client.clone();

    let screen = AlternateScreen::from(stdout());
    home_screen();

    // Start consuming incoming messages, otherwise they will back up.
    //
    // First tokio task to listen for incoming server messages
    // and format them as needed before printing them to the console.
    let join_handle = tokio::spawn(async move {
        loop {
            select! {
                Some(message) = incoming_messages.recv() => {
                    print_message(format_message(message).await, input_buffer2.read().await.to_string()).await;
                    if input_buffer2.read().await.is_empty() {
                        user_interface::empty_line();
                    }
                },
                // End process if sender message received.
                _ = shutdown_rx.recv() => break,
            };
        }
    });

    // Use this channel to send/receive termion::Event::Key
    let (input_tx, input_rx) = mpsc::channel();

    // Second tokio task to listen to user input and outgoing chat messages.
    let join_handle2 = tokio::spawn(async move {
        // Set terminal to raw mode to allow reading
        // stdin one key at a time.
        let mut stdout = stdout().into_raw_mode().unwrap();
        let mut stdin = termion::async_stdin().keys();
        let mut buffer_position = input_buffer.read().await.len();

        loop {
            let key = loop {
                match input_rx.try_recv() {
                    Err(TryRecvError::Empty) => {}, // no op, keep trying to read from channel
                    Err(TryRecvError::Disconnected) => unimplemented!(), // What should we do if one part of the channel disconnects?
                    Ok(code) => break code,
                }
                task::yield_now().await;
            };

            // TODO: Look into thread spawn or tokio::Stdin, .next() poss blocking main thread.
            // TODO: Also look into poss channel, maybe abstract some of the pattern 
            // matching?
            let first_char = input_buffer.read().await.chars().nth(0);
            match key {
                termion::event::Key::Char('\n') => {
                    if !input_buffer.read().await.is_empty() {
                        if first_char == Some(':') {
                            // If the entered input buffer starts with a ':'
                            // then the run_command function is executed,
                            // parsing the command and running its logic.
                            command_tx.send(()).ok();
                        } else {
                            send_user_message(
                                user_name.read().await.as_str(),
                                current_channel.read().await.as_str(),
                                Arc::clone(&input_buffer),
                                &client2,
                            )
                                .await;
                        }
                        user_interface::empty_line();
                    }
                }
                termion::event::Key::Char(user_input) => {
                    // write user input to the console
                    // and save it to input_buffer
                    if !input_buffer.read().await.is_empty() {
                        write!(stdout, "{}", user_input).unwrap();
                    } else {
                        write!(stdout, "{}{}", termion::clear::AfterCursor, user_input)
                            .unwrap();
                    }
                    input_buffer.write().await.push(user_input);
                    buffer_position += 1;
                }
                termion::event::Key::Left => {
                    if buffer_position == 0 {
                    } else {
                        write!(stdout, "{}", termion::cursor::Left(1)).unwrap();
                        buffer_position -= 1;
                    }
                }
                termion::event::Key::Right => {
                    if buffer_position == input_buffer.read().await.len() {
                    } else {
                        write!(stdout, "{}", termion::cursor::Right(1)).unwrap();
                        buffer_position += 1;
                    }
                }
                termion::event::Key::Backspace => {
                    // Backspace does nothing unless the input_buffer
                    // has characters to delete.
                    if !input_buffer.read().await.is_empty() {
                        input_buffer.write().await.pop();
                        write!(
                            stdout,
                            "{}{}",
                            termion::cursor::Left(1),
                            termion::clear::AfterCursor
                        )
                            .unwrap();
                        buffer_position -= 1;
                        if input_buffer.read().await.is_empty() {
                            user_interface::empty_line();
                        }
                    }
                }
                termion::event::Key::Ctrl('q') => {
                    // Send message to receivers to end process.
                    shutdown_tx.send(()).ok();
                    break;
                }

                _ => {}
            }
            stdout.lock().flush().unwrap();
        }
    });

    let join_handle3 = tokio::spawn(async move {
        loop {
            select! {
                    // if a command ':' is found in a sent input buffer,
                    // call run_command to parse the input and handle the command
                    Ok(_command) = command_rx.recv() => {
                        run_command(
                            Arc::clone(&input_buffer_lock),
                            Arc::clone(&current_channel_read),
                            &config_path,
                            &client
                        ).await
                },
                     // End process if sender message received.
                    _ = shutdown_rx2.recv() => break,
            };
        }
    });

    let input_reader_tx = input_tx.clone();
    let _input_reader = std::thread::spawn(move || {
        loop {
            let keys = std::io::stdin().keys();
            for key in keys {
                // `key` can be an Err variant, are we gonna handle those?
                if let Ok(code) = key {
                    if let Err(_error) = input_reader_tx.send(code) {
                        // What do we do if an error happens?
                        unimplemented!();
                    };
                }
            }
        }
    });

    // Keep the tokio executor alive.
    // If you return instead of waiting,
    // the background task will exit.
    futures::join!(join_handle, join_handle2, join_handle3);
    screen.lock().flush().unwrap();
    Ok(())
}
