use twilight_cache_inmemory::model::CachedVoiceState;
use twilight_model::id::{
	marker::{ChannelMarker, GuildMarker, UserMarker},
	Id,
};

use crate::{Bot, Permissions, MONITORED_CHANNEL_TYPES};

/// Search for users in voice channels that should be removed.
#[derive(Clone, Copy)]
pub struct Search {
	bot: Bot,
	guild: Id<GuildMarker>,
}

impl Search {
	/// Not directly used, see [`Bot::search`].
	///
	/// [`Bot::search`]: Bot::search
	pub const fn new(bot: Bot, guild: Id<GuildMarker>) -> Self {
		Self { bot, guild }
	}

	/// Returns `true` if the user is permitted to be in the voice channel.
	fn is_permitted(&self, state: &CachedVoiceState) -> bool {
		self.bot
			.cache
			.permissions()
			.in_channel(state.user_id(), state.channel_id())
			.expect("resources are available")
			.contains(Permissions::CONNECT)
	}

	/// Returns a list of [`Id<UserMarker>`]'s to be removed.
	pub fn channel(self, channel: Id<ChannelMarker>) -> Vec<Id<UserMarker>> {
		match self.bot.cache.voice_channel_states(channel) {
			Some(state) => state
				.into_iter()
				.filter_map(|state| (!self.is_permitted(&state)).then(|| state.user_id()))
				.collect(),
			None => Vec::new(),
		}
	}

	/// Returns a list of [`Id<UserMarker>`]'s to be removed.
	pub fn guild(self) -> Vec<Id<UserMarker>> {
		let channels = self.bot.cache.guild_channels(self.guild).expect("cached");

		channels
			.iter()
			.filter_map(|&id| {
				(MONITORED_CHANNEL_TYPES
					.contains(&self.bot.cache.channel(id).expect("cached").kind)
					&& self.bot.is_monitored(id))
				.then(|| self.channel(id))
			})
			.flatten()
			.collect()
	}

	/// Returns `true` if a [`Id<UserMarker>`] should be removed.
	pub fn user(self, user: Id<UserMarker>) -> bool {
		matches!(self.bot.cache.voice_state(user, self.guild), Some(s) if !self.is_permitted(&s))
	}
}
