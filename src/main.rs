use std::collections::HashMap;
use std::sync::Arc;

use commands::general::*;
use commands::music::*;
use commands::owner::*;
use poise::serenity_prelude::GuildId;
use poise::serenity_prelude::MessageId;
use poise::serenity_prelude::UserId;
use poise::serenity_prelude::{self as serenity, Mutex};
use songbird::Songbird;
use songbird::SongbirdKey;
use tracing::{error, info};
use utils::check_result;
use uuid::Uuid;

mod commands;
mod configuration;
mod error;
mod model;
mod music;
mod utils;
mod youtube;

#[derive(Clone)]
pub struct Data {
    config: Arc<Mutex<configuration::Config>>,
    song_queues: Arc<Mutex<HashMap<Uuid, UserId>>>,
    song_messages: Arc<Mutex<HashMap<GuildId, MessageId>>>,
    song_status: Arc<Mutex<HashMap<GuildId, bool>>>,
}
pub type Error = error::AyameError;

pub type Context<'a> = poise::Context<'a, Data, Error>;

async fn event_listener(
    _ctx: &serenity::Context,
    event: &poise::Event<'_>,
    _framework: &poise::Framework<Data, Error>,
    _data: &Data,
) -> Result<(), Error> {
    match event {
        poise::Event::Ready { data_about_bot } => {
            info!("{} is connected!", data_about_bot.user.name)
        }
        _ => {}
    }

    Ok(())
}

#[poise::command(prefix_command, slash_command)]
async fn help(
    ctx: Context<'_>,
    #[description = "Command to display specific information about"] command: Option<String>,
) -> Result<(), Error> {
    let config = poise::builtins::HelpConfiguration {
        extra_text_at_bottom: "\
If you want more information about a specific command, just pass the command as argument.",
        ..Default::default()
    };

    poise::builtins::help(ctx, command.as_deref(), config).await?;

    Ok(())
}

async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    match error {
        poise::FrameworkError::Command { error, ctx } => {
            if let Error::Input(_) = error {
                error.send_error(&ctx).await;
            } else {
                check_result(
                    ctx.say(format!("failure: {:?}\nContact bot owner", error))
                        .await,
                );
                error!("Error while executing command: {:?}", error)
            }
        }
        poise::FrameworkError::Listener { error, event } => {
            error!(
                "Listener returned error during {:?} event: {:?}",
                event.name(),
                error
            );
        }
        error => {
            if let Err(e) = poise::builtins::on_error(error).await {
                error!("Error while handling error: {}", e)
            }
        }
    }
}

#[tokio::main]
async fn main() {
    if let Err(_) = std::env::var("RUST_LOG") {
        std::env::set_var("RUST_LOG", "INFO");
    }
    tracing_subscriber::fmt::init();
    let config = configuration::config();
    let options = poise::FrameworkOptions {
        commands: vec![
            help(),
            mock(),
            register(),
            unregister(),
            mensa(),
            invite(),
            join(),
            leave(),
            play(),
            skip(),
            shutdown(),
            addemote(),
        ],
        listener: |ctx, event, framework, user_data| {
            Box::pin(event_listener(ctx, event, framework, user_data))
        },
        on_error: |error| Box::pin(on_error(error)),
        // Options specific to prefix commands, i.e. commands invoked via chat messages
        prefix_options: poise::PrefixFrameworkOptions {
            prefix: Some(String::from(config.prefix())),

            mention_as_prefix: true,
            // An edit tracker needs to be supplied here to make edit tracking in commands work
            edit_tracker: Some(poise::EditTracker::for_timespan(
                std::time::Duration::from_secs(3600 * 3),
            )),
            ..Default::default()
        },
        ..Default::default()
    };

    poise::Framework::build()
        .client_settings(move |client_builder: serenity::ClientBuilder| {
            // get songbird instance
            let voice = Songbird::serenity();
            client_builder
                // TODO: lazy so use all intents
                .intents(serenity::GatewayIntents::all())
                // register songbird as VoiceGatewayManager
                .voice_manager_arc(voice.clone())
                // insert songbird into data
                .type_map_insert::<SongbirdKey>(voice)
        })
        .token(config.token())
        .user_data_setup(|ctx, _data_about_bot, _framework| {
            Box::pin(async move {
                // set activity to "{prefix}help"
                ctx.set_activity(serenity::Activity::listening(format!(
                    "{}help",
                    config.prefix()
                )))
                .await;
                // store config in Data
                Ok(Data {
                    config: Arc::new(Mutex::new(config)),
                    song_queues: Arc::new(Mutex::new(HashMap::new())),
                    song_messages: Arc::new(Mutex::new(HashMap::new())),
                    song_status: Arc::new(Mutex::new(HashMap::new())),
                })
            })
        })
        .options(options)
        .run()
        .await
        .expect("Client error");
}
