//! Handles role events

use tracing::{event, instrument, Level};
use twilight_model::{
	channel::VoiceChannel,
	guild::Permissions,
	id::{GuildId, RoleId},
};

use crate::{permission, Bot, InMemoryCacheExt};

use super::remove_user;

pub struct Handler {
	bot: &'static Bot,
	guild_id: GuildId,
}

impl Handler {
	pub const fn new(bot: &'static Bot, guild_id: GuildId) -> Self {
		Self { bot, guild_id }
	}

	// 1. get list of voice channels
	// 2. check for move permission - wait upstreaming permission into cache
	// 3. get users in channels
	// 4. check em (only ones with the changed role)
	#[instrument(skip(self), fields(guild_id = %self.guild_id))]
	pub async fn process(self, role_id: Option<RoleId>) {
		let managed_channels = self.managed_channels();

		for (channel, voice_states) in managed_channels.map(|channel| {
			let channel_id = channel.id;
			(
				channel,
				self.bot
					.cache
					.voice_channel_states(channel_id)
					.unwrap_or_default(),
			)
		}) {
			event!(Level::DEBUG, %channel.id, "searching through channel");
			for voice_state in voice_states.into_iter().filter(|state| {
				if let (Some(role_id), Some(member)) = (role_id, state.member.as_ref()) {
					// member.roles doesn't contain everybody role
					member.roles.contains(&role_id) || role_id == self.guild_id.0.into()
				} else {
					// if role is unset don't filter anybody
					true
				}
			}) {
				event!(Level::DEBUG, %voice_state.user_id, "checking user");
				if !permission(
					&channel,
					self.guild_id,
					voice_state.user_id,
					&self.bot.cache,
					Permissions::CONNECT,
				)
				.unwrap_or(true)
				{
					remove_user(self.bot, self.guild_id, voice_state.user_id).await
				}
			}
		}
	}

	/// Get all "managed" [`VoiceChannel`]s in a guild.
	///
	/// Managed means that the bot has [`Permissions::MOVE_MEMBERS`] in it.
	fn managed_channels(&self) -> impl Iterator<Item = VoiceChannel> + '_ {
		self.bot
			.cache
			.voice_channels(self.guild_id)
			.expect("valid guild_id")
			.into_iter()
			.filter(move |channel| {
				permission(
					channel,
					self.guild_id,
					self.bot.id,
					&self.bot.cache,
					Permissions::MOVE_MEMBERS,
				)
				.unwrap_or(false)
			})
	}
}
