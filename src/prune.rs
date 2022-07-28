//! Search through resources for users who should be pruned.

use twilight_cache_inmemory::model::CachedVoiceState;
use twilight_model::{
	guild::Permissions,
	id::{
		marker::{ChannelMarker, GuildMarker, UserMarker},
		Id,
	},
};

use crate::BOT;

fn is_permitted(state: &CachedVoiceState) -> bool {
	BOT.cache
		.permissions()
		.in_channel(state.user_id(), state.channel_id())
		.expect("resources are available")
		.contains(Permissions::CONNECT)
}

pub async fn channel(channel: Id<ChannelMarker>, guild: Id<GuildMarker>) -> u16 {
	let users = BOT
		.is_monitored(channel)
		.then(|| {
			BOT.cache
				.voice_channel_states(channel)
				.map_or(Vec::new(), |states| {
					states
						.into_iter()
						.filter_map(|state| (!is_permitted(&state)).then(|| state.user_id()))
						.collect()
				})
		})
		.unwrap_or_default();

	BOT.remove(guild, users.into_iter()).await
}

pub async fn guild(guild: Id<GuildMarker>) -> u16 {
	let channels = BOT.cache.guild_channels(guild).expect("cached");

	// FIXME: replace with async closure once stable
	futures_util::future::join_all(channels.iter().map(|&id| channel(id, guild)))
		.await
		.into_iter()
		.sum()
}

pub async fn user(guild: Id<GuildMarker>, user: Id<UserMarker>) {
	if matches!(BOT.cache.voice_state(user, guild), Some(s) if !is_permitted(&s)) {
		BOT.remove(guild, Some(user)).await;
	}
}
