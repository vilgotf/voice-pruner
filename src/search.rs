use twilight_cache_inmemory::model::CachedVoiceState;
use twilight_model::id::{
	marker::{ChannelMarker, GuildMarker, UserMarker},
	Id,
};

use crate::{Permissions, BOT, MONITORED_CHANNEL_TYPES};

/// Search for users in voice channels that should be removed.
#[derive(Clone, Copy)]
pub struct Search {
	guild: Id<GuildMarker>,
}

impl Search {
	/// Not directly used, see [`Bot::search`].
	///
	/// [`Bot::search`]: crate::Bot::search
	pub const fn new(guild: Id<GuildMarker>) -> Self {
		Self { guild }
	}

	/// Returns `true` if the user is permitted to be in the voice channel.
	fn is_permitted(&self, state: &CachedVoiceState) -> bool {
		BOT.cache
			.permissions()
			.in_channel(state.user_id(), state.channel_id())
			.expect("resources are available")
			.contains(Permissions::CONNECT)
	}

	/// Returns a list of [`Id<UserMarker>`]'s to be removed.
	pub fn channel(self, channel: Id<ChannelMarker>) -> Vec<Id<UserMarker>> {
		match BOT.cache.voice_channel_states(channel) {
			Some(state) => state
				.into_iter()
				.filter_map(|state| (!self.is_permitted(&state)).then(|| state.user_id()))
				.collect(),
			None => Vec::new(),
		}
	}

	/// Returns a list of [`Id<UserMarker>`]'s to be removed.
	pub fn guild(self) -> Vec<Id<UserMarker>> {
		let channels = BOT.cache.guild_channels(self.guild).expect("cached");

		channels
			.iter()
			.filter_map(|&id| {
				(MONITORED_CHANNEL_TYPES.contains(&BOT.cache.channel(id).expect("cached").kind)
					&& BOT.is_monitored(id))
				.then(|| self.channel(id))
			})
			.flatten()
			.collect()
	}

	/// Returns `true` if a [`Id<UserMarker>`] should be removed.
	pub fn user(self, user: Id<UserMarker>) -> bool {
		matches!(BOT.cache.voice_state(user, self.guild), Some(s) if !self.is_permitted(&s))
	}
}
