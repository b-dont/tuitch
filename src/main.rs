#![allow()]

use crate::commands::run_command;
use crate::messages::{format_message, print_message, send_user_message};
use crate::user_config::{get_client_config, set_client_config};
use crate::user_interface::home_screen;
use owo_colors::OwoColorize;
use std::{io::stdout, io::Write, sync::Arc};
use termion::{input::TermRead, raw::IntoRawMode, screen::AlternateScreen};
use tokio::{select, sync::broadcast, sync::RwLock};
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
    let placeholder: &str = "Enter a message or command";

    // Input-buffer for user's typed input and chat messages.
    // This is a shared state to allow proper handling with incoming
    // server messages while unsent user input is in the console.
    let input_buffer_lock = Arc::new(RwLock::new(String::new()));
    let input_buffer = Arc::clone(&input_buffer_lock);
    let input_buffer2 = Arc::clone(&input_buffer_lock);

    // Create tx/rx to send and receive shutdown signal
    // when specific user input is detected.
    let (shutdown_tx, mut shutdown_rx) = broadcast::channel(1);
    let _shutdown_rx2 = shutdown_tx.subscribe();
    let _shutdown_rx3 = shutdown_tx.subscribe();

    // Channel for chat-line commands and settings.
    let (command_tx, _command_rx) = broadcast::channel(2);
    let mut command_rx = command_tx.subscribe();

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
    home_screen().await;

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
                        print!(
                            "\r> {}\r{}",
                            &placeholder.dimmed(),
                            termion::cursor::Right(2)
                           );
                        stdout().lock().flush().unwrap();
                       }
                },
                // End process if sender message received.
                _ = shutdown_rx.recv() => break,
            };
        }
    });

    // Second tokio task to listen to user input and outgoing chat messages.
    let join_handle2 = tokio::spawn(async move {
        // Set terminal to raw mode to allow reading
        // stdin one key at a time.
        let mut stdout = stdout().into_raw_mode().unwrap();
        let mut stdin = termion::async_stdin().keys();

        loop {
            let input = stdin.next();
            let first_char = input_buffer.read().await.chars().nth(0);
            if let Some(Ok(key)) = input {
                match key {
                    termion::event::Key::Esc => {
                        // Send message to receivers to end process.
                        shutdown_tx.send(()).ok();
                        break;
                    }
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
                            write!(
                                stdout,
                                "\r> {}\r{}",
                                &placeholder.dimmed(),
                                termion::cursor::Right(2)
                            ).unwrap();
                        }
                    }
                    termion::event::Key::Char(user_input) => {
                        // write user input to the console
                        // and save it to input_buffer
                        if !input_buffer.read().await.is_empty() {
                            write!(stdout, "{}", user_input).unwrap();
                        } else {
                            write!(stdout, "{}\r> {}", termion::clear::CurrentLine, user_input).unwrap();
                        }
                        input_buffer.write().await.push(user_input);
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
                            ).unwrap();
                            if input_buffer.read().await.is_empty(){
                                write!(
                                    stdout,
                                    "\r> {}\r{}",
                                    &placeholder.dimmed(),
                                    termion::cursor::Right(2)
                                ).unwrap();
                            }
                        }
                    }
                    _ => {}
                }
                stdout.lock().flush().unwrap();
            }
        }
    });

    let join_handle3 = tokio::spawn(async move {
        loop {
            select! {
                // if a command ':' is found in a sent input buffer,
                // call run_command to parse the input and handle the command
                Ok(_command) = command_rx.recv() => run_command(
                    Arc::clone(&input_buffer_lock),
                    Arc::clone(&current_channel_read),
                    &config_path,
                    &client
                    ).await
            };
        }
    });

    // Keep the tokio executor alive.
    // If you return instead of waiting,
    // the background task will exit.
    futures::join!(join_handle, join_handle2, join_handle3);
    screen.lock().flush().unwrap();
    Ok(())
}
