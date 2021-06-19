//
// just some testing, to find out what works best
//
use serenity::{
    framework::standard::CommandResult,
    model::{
        channel::Channel,
        id::{ChannelId, GuildId},
    },
};

// handles music for a guild
pub struct MusicManager<T, E, P>
where
    T: MusicPlayer,
    E: MusicContext,
    P: MusicParser,
{
    player: T,
    context: E,
    parser: P,
    guild_id: GuildId,
    binding: Binding,
}

pub enum Binding {
    UNBOUND,
    BOUND(ChannelId),
}

impl<T, E, P> MusicManager<T, E, P>
where
    T: MusicPlayer,
    E: MusicContext,
    P: MusicParser,
{
    fn bind(&mut self, new_binding: Binding) {
        self.binding = new_binding;
    }
}

// trait to handle search querries
pub trait MusicPlayer {
    fn play(&self, video: dyn Video);
}

pub trait MusicParser {
    fn search(&self, link: &str) -> dyn Video;
}

// trait to handle output
pub trait MusicContext {
    fn queued_song(&self, channel: &Channel) -> CommandResult;
}

// trait to represent a basic video
pub trait Video {
    fn link(&self) -> &str;
}
