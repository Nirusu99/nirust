// commands
mod commands;

mod model;

use model::youtubedl;
use serenity::{
    async_trait,
    client::bridge::gateway::ShardManager,
    framework::{standard::macros::group, StandardFramework},
    http::Http,
    model::{
        event::ResumedEvent,
        gateway::Ready,
        interactions::{
            application_command::{
                ApplicationCommand, ApplicationCommandInteractionDataOptionValue,
                ApplicationCommandOptionType,
            },
            Interaction, InteractionResponseType,
        },
    },
    prelude::*,
};

use std::path::PathBuf;
use std::{collections::HashSet, env, fs::remove_dir_all, sync::Arc};

use config::*;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

use commands::{admin::*, general::*};

use lazy_static::*;

lazy_static! {
    pub static ref CONFIG: Config = {
        let mut settings = Config::default();
        settings
            .merge(File::with_name(
                get_file("config.yml")
                    .to_str()
                    .expect("Couldn't get path of bot dir"),
            ))
            .expect("Expected config.yml in bot directory");

        settings
    };
    pub static ref BOT_DIR: PathBuf = {
        let mut dir = std::env::current_exe().expect("Couldn't get bot directory");
        dir.pop();
        dir
    };
}

pub struct ShardManagerContainer;

// shard manager
impl TypeMapKey for ShardManagerContainer {
    type Value = Arc<Mutex<ShardManager>>;
}

struct Handler;

// Ready and Resumed events to notify if the bot has started/resumed
#[async_trait]
impl EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(command) = interaction {
            let content = match command.data.name.as_str() {
                "youtubedl" => {
                    let options = command
                        .data
                        .options
                        .get(0)
                        .expect("Expected user option")
                        .resolved
                        .as_ref()
                        .expect("Expected String object");
                    if let ApplicationCommandInteractionDataOptionValue::String(url) = options {
                        let mut content = "Recieved URL".to_string();
                        if crate::commands::general::URL_REGEX.is_match(url) {
                            tokio::spawn(youtubedl::start_download(
                                command.channel_id.clone(),
                                command.user.id.as_u64().clone(),
                                ctx.http.clone(),
                                url.to_string(),
                            ));
                        } else {
                            content = "Invalid URL".to_string();
                        }
                        content
                    } else {
                        "Expect URL".to_string()
                    }
                }
                _ => "Not implemented :(".to_string(),
            };
            if let Err(why) = command
                .create_interaction_response(&ctx.http, |response| {
                    response
                        .kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|message| message.content(content))
                })
                .await
            {
                println!("Cannot respond to slash command: {}", why);
            }
        }
    }
    async fn ready(&self, ctx: Context, ready: Ready) {
        let commands = ApplicationCommand::set_global_application_commands(&ctx.http, |commands| {
            commands.create_application_command(|command| {
                command
                    .name("youtubedl")
                    .description("Download Videos from a lot of sites")
                    .create_option(|option| {
                        option
                            .name("link")
                            .description("A link to a video source")
                            .kind(ApplicationCommandOptionType::String)
                            .required(true)
                    })
            })
        })
        .await;

        println!(
            "I now have the following global slash commands {:#?}",
            commands
        );
        info!("Connected as {}", ready.user.name);

        let _ = ApplicationCommand::create_global_application_command(&ctx.http, |a| {
            a.name("ping").description("A simple ping command")
        })
        .await;

        let interactions = ApplicationCommand::get_global_application_commands(&ctx.http).await;

        println!(
            "I have the following global slash command(s): {:?}",
            interactions
        );
    }

    async fn resume(&self, _: Context, _: ResumedEvent) {
        info!("Resumed");
    }
}

#[group]
#[commands(ping, ytd)]
struct General;

#[group]
#[commands(addemote)]
struct Admin;

pub fn get_file(name: &str) -> PathBuf {
    let mut dir = BOT_DIR.clone();
    dir.push(name);
    dir
}

#[tokio::main]
async fn main() {
    // load environment
    dotenv::dotenv().expect("Failed to load environment");

    // init the logger to use environment variables
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to start the Logger");

    // get token from .evv
    let token = env::var("DISCORD_TOKEN").map_or(
        // or from config.yml
        CONFIG
            .get_str("token")
            .expect("Failed to load DISCORD_TOKEN from the environment"),
        |m| m.to_string(),
    );

    let http = Http::new_with_token(&token);

    // get owners and bot id from application
    let (owners, bot_id) = match http.get_current_application_info().await {
        Ok(info) => {
            let mut owners = HashSet::new();
            owners.insert(info.owner.id);

            (owners, info.id)
        }
        Err(why) => panic!("Could not access application info: {:?}", why),
    };

    // Create bot
    //load bot prefix from config
    let prefix: &str = &CONFIG
        .get_str("prefix")
        .expect("Couldn't find bot prefix in config");

    info!("Cleaning temporary directory");
    let _ = remove_dir_all(get_file("tmp"));

    let framework = StandardFramework::new()
        .configure(|c| {
            c.owners(owners)
                .prefix(prefix)
                .on_mention(Some(bot_id))
                .with_whitespace(true)
                .delimiters(vec![", ", ","])
                .no_dm_prefix(true)
        })
        .group(&GENERAL_GROUP)
        .group(&ADMIN_GROUP)
        // annote command with #[bucket = "really_slow"]
        // to limit command usage to 1 uses per 10 minutes
        .bucket("really_slow", |b| b.time_span(600).limit(1))
        .await;
    let application_id: u64 = env::var("APPLICATION_ID")
        .map_or(
            // or from config.yml
            CONFIG
                .get_str("application_id")
                .expect("Failed to load APPLICATION_ID from the environment"),
            |m| m.to_string(),
        )
        .parse()
        .expect("application id is not a valid id");

    let mut client = Client::builder(&token)
        .framework(framework)
        .event_handler(Handler)
        .application_id(application_id)
        .await
        .expect("Err creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<ShardManagerContainer>(client.shard_manager.clone());
    }

    let shard_manager = client.shard_manager.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Could not register ctrl+c handler");
        shard_manager.lock().await.shutdown_all().await;
    });

    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why);
    }
}
