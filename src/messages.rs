use chrono::{DateTime, Timelike, Utc};
use owo_colors::{OwoColorize, Rgb};
use std::{
    io::{stdout, Write},
    sync::Arc,
};
use tokio::sync::RwLock;
use twitch_irc::message::{
    ClearChatAction, HostTargetAction, RGBColor, ServerMessage, UserNoticeEvent,
};
use twitch_irc::{
    login::StaticLoginCredentials, /*ClientConfig,*/ SecureTCPTransport, TwitchIRCClient,
};

// TODO: Look into adding emotes.
// TODO: Text alignment.
// TODO: Window sizing.
// TODO: Scrolling through chat history.
// TODO: Look into chat replies from users.

pub async fn format_message(message: ServerMessage) -> Option<String> {
    match message {
        // User chat messages:
        ServerMessage::Privmsg(prvmsg) => {
            let user_name_color = prvmsg.name_color.unwrap_or(RGBColor {
                r: 255,
                g: 255,
                b: 255,
            });
            Some(format!(
                " {} {}: {}\n",
                prvmsg.server_timestamp.format("%H:%M").dimmed(),
                prvmsg.sender.name.bold().color(Rgb(
                    user_name_color.r,
                    user_name_color.g,
                    user_name_color.b
                )),
                prvmsg.message_text
            ))
        }

        // User time-outs, bans, and a cleared chat history messages:
        ServerMessage::ClearChat(clearchat) => match clearchat.action {
            ClearChatAction::UserBanned { user_login, .. } => {
                Some(format!("{name} has been banned.", name = user_login))
            }
            ClearChatAction::UserTimedOut {
                user_login,
                timeout_length,
                ..
            } => Some(format!(
                "{name} has been timed-out for {seconds} seconds.",
                name = user_login,
                seconds = timeout_length.as_secs()
            )),
            ClearChatAction::ChatCleared => Some(format!("Chat has been cleared.")),
        },

        // Channel-hosting
        ServerMessage::HostTarget(hosttargetmessage) => match hosttargetmessage.action {
            HostTargetAction::HostModeOn {
                hosted_channel_login,
                viewer_count,
            } => {
                let viewer_count = viewer_count.unwrap_or(0);
                Some(format!(
                    "Hosted {} with {} users",
                    hosted_channel_login, viewer_count
                ))
            }
            HostTargetAction::HostModeOff { .. } => Some(format!("No longer hosting.")),
        },

        // Event messages, raids, subs:
        // TODO: Fix sub tier text
        ServerMessage::UserNotice(usernotice) => match usernotice.event {
            UserNoticeEvent::SubOrResub {
                is_resub,
                cumulative_months,
                streak_months,
                sub_plan,
                sub_plan_name,
            } => {
                if is_resub {
                    Some(format!(
                        "{name} has subscribed for {months} months with {plan}!",
                        name = usernotice.sender.name,
                        months = cumulative_months,
                        plan = sub_plan,
                    ))
                } else {
                    Some(format!(
                        "{name} has just subscribed with {plan}!",
                        name = usernotice.sender.name,
                        plan = sub_plan,
                    ))
                }
            }

            UserNoticeEvent::Raid {
                viewer_count,
                profile_image_url,
            } => Some(format!(
                "{name} raided with {viewers} viewers!",
                name = usernotice.sender.name,
                viewers = viewer_count,
            )),

            UserNoticeEvent::SubGift {
                is_sender_anonymous,
                cumulative_months,
                recipient,
                sub_plan,
                sub_plan_name,
                num_gifted_months,
            } => {
                if is_sender_anonymous {
                    Some(format!(
                        "An anonymous user gifted {} a {} for {:?}!",
                        recipient.name, sub_plan, num_gifted_months,
                    ))
                } else {
                    Some(format!(
                        "{} gifted {} a {} for {:?}!",
                        usernotice.sender.name, recipient.name, sub_plan, num_gifted_months,
                    ))
                }
            }

            UserNoticeEvent::SubMysteryGift {
                mass_gift_count,
                sender_total_gifts,
                sub_plan,
            } => Some(format!(
                "{} is gifting {} subs! They've gifted a total of {}!",
                usernotice.sender.name, mass_gift_count, sender_total_gifts,
            )),

            UserNoticeEvent::AnonSubMysteryGift {
                mass_gift_count,
                sub_plan,
            } => Some(format!(
                "An anonymous user is gifting {} subs!",
                mass_gift_count
            )),

            UserNoticeEvent::GiftPaidUpgrade {
                gifter_login,
                gifter_name,
                promotion,
            } => Some(format!(
                "{} continued their gifted sub from {}!",
                usernotice.sender.name, gifter_name
            )),

            UserNoticeEvent::AnonGiftPaidUpgrade { promotion } => Some(format!(
                "{} continued their gifted sub from an anonymous user!",
                usernotice.sender.name
            )),

            UserNoticeEvent::Ritual { ritual_name } => Some(format!(
                "{} is new to chat! Say hi!",
                usernotice.sender.name
            )),

            UserNoticeEvent::BitsBadgeTier { threshold } => Some(format!(
                "{} just earned the {} bits badge!",
                usernotice.sender.name, threshold
            )),

            _ => None,
        },

        // Simple server messages related to user and moderator actions and
        // server-side messages:

        // TODO: Look into and proper message removal.
        // Will probably need to buffer n incoming messages.
        ServerMessage::ClearMsg(_) => Some(format!("Message deleted.\n")),
        ServerMessage::GlobalUserState(_) => Some(format!("Login successful!\n")),
        ServerMessage::Part(_) => Some(format!("Departed chat.\n")),
        ServerMessage::Notice(notice) => Some(format!("{}\n", notice.message_text)),
        ServerMessage::Join(join) => Some(format!("Joined {}'s chat!\n\n", join.channel_login)),

        // Any other events that do not need to be verbose
        _ => None,
    }
}

pub async fn print_message(server_message: Option<String>, input_buffer: String) {
    // When a server message is printed, the user's current input
    // and the UI need to be taken into account.
    //
    // The current line must be cleared first, and carraige return called.
    // The message is then printed, and carraige return and newline are called.
    // If the user had any unsent input, that is saved and printed
    // back to the console, then the cursor is restored to it's previous
    // position.
    if let Some(message) = server_message {
        print!(
            "{clear}\r{msg}\r\n",
            clear = termion::clear::CurrentLine,
            msg = message,
        );
        print!("> {}", input_buffer);
    }
    stdout().flush().unwrap();
}

pub async fn send_user_message(
    username: &str,
    current_channel: &str,
    input_buffer: Arc<RwLock<String>>,
    client: &TwitchIRCClient<SecureTCPTransport, StaticLoginCredentials>,
) {
    let mut buffer = input_buffer.write().await;
    let now: DateTime<Utc> = Utc::now();
    client
        .privmsg(current_channel.to_owned(), buffer.to_owned())
        .await
        .unwrap();
    print!(
        "{}\r {}:{} {}: {}\n\n",
        termion::clear::CurrentLine,
        now.hour().dimmed(),
        now.minute().dimmed(),
        username.bold(),
        buffer.to_string()
    );
    buffer.clear();
}
