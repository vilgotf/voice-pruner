//! Search through resources for users who should be pruned.

use futures_util::{stream, StreamExt};
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

/// Prune users in the channel that are not permitted and where the `kick` closure returns `true`.
pub async fn channel<F>(channel: Id<ChannelMarker>, guild: Id<GuildMarker>, kick: F) -> u16
where
	F: Fn(&CachedVoiceState) -> bool,
{
	let users = BOT
		.is_monitored(channel)
		.then(|| {
			BOT.cache
				.voice_channel_states(channel)
				.map_or(Vec::new(), |states| {
					states
						.into_iter()
						.filter_map(|state| {
							(!is_permitted(&state) && kick(&state)).then(|| state.user_id())
						})
						.collect()
				})
		})
		.unwrap_or_default();

	BOT.remove(guild, users.into_iter()).await
}

/// Prune users in the guild that are not permitted and where the `kick` closure returns `true`.
pub async fn guild<F>(guild: Id<GuildMarker>, kick: F) -> u16
where
	F: Fn(&CachedVoiceState) -> bool + Copy,
{
	let channels = BOT.cache.guild_channels(guild).expect("cached");

	stream::iter(channels.iter())
		.map(|&id| channel(id, guild, kick))
		.fold(0, |a, b| async move { a + b.await })
		.await
}

pub async fn user(guild: Id<GuildMarker>, user: Id<UserMarker>) {
	if matches!(BOT.cache.voice_state(user, guild), Some(state) if !is_permitted(&state)) {
		BOT.remove(guild, Some(user)).await;
	}
}
