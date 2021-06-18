// commands
mod commands;
// utility
mod util;

pub mod model;

use serenity::{
    async_trait,
    client::bridge::gateway::ShardManager,
    framework::{standard::macros::group, StandardFramework},
    http::Http,
    model::{event::ResumedEvent, gateway::Ready},
    prelude::*,
};
use std::path::PathBuf;
use std::{collections::HashSet, env, fs::remove_dir_all, sync::Arc};

use config::*;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

use commands::{admin::*, general::*, youtubedl::*};

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
    async fn ready(&self, _: Context, ready: Ready) {
        info!("Connected as {}", ready.user.name);
    }

    async fn resume(&self, _: Context, _: ResumedEvent) {
        info!("Resumed");
    }
}

#[group]
#[commands(ping)]
struct General;

#[group]
#[commands(ytd)]
struct YoutubeDL;

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
        .group(&YOUTUBEDL_GROUP)
        .group(&ADMIN_GROUP)
        // annote command with #[bucket = "basic"]
        // to limit command usage to 3 uses per 10 secs with a 2 seconds delay
        // between invocations
        .bucket("basic", |b| b.delay(2).time_span(10).limit(3))
        .await;

    let mut client = Client::builder(&token)
        .framework(framework)
        .event_handler(Handler)
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
