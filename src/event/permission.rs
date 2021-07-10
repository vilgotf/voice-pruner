use tracing::{event, instrument, Level};
use twilight_model::{
	channel::{GuildChannel, VoiceChannel},
	guild::Permissions,
	id::{ChannelId, GuildId, RoleId, UserId},
};

use crate::{permission, Bot, InMemoryCacheExt};

pub struct Permission {
	bot: &'static Bot,
	guild_id: GuildId,
	mode: Mode,
}

#[derive(Debug)]
pub enum Mode {
	Channel(ChannelId),
	Member(UserId),
	Role(Option<RoleId>),
}

/// Tries to remove a user from voice channel in a new task, logging on error.
async fn remove_user(bot: &'static Bot, guild_id: GuildId, user_id: UserId) {
	event!(Level::INFO, member.user.id = %user_id, "kicking user");
	if let Err(e) = bot
		.http
		.update_guild_member(guild_id, user_id)
		.channel_id(None)
		.await
	{
		// Workaround tracing::Value being weird tokio-rs/tracing#1308
		event!(Level::ERROR, error = &e as &dyn std::error::Error);
	}
}

impl Permission {
	pub const fn new(bot: &'static Bot, guild_id: GuildId, mode: Mode) -> Self {
		Self {
			bot,
			guild_id,
			mode,
		}
	}

	#[instrument(skip(self), fields(guild_id = %self.guild_id))]
	pub async fn act(&self) {
		let users = match self.mode {
			Mode::Channel(c) => self.channel(c),
			Mode::Member(user_id) => self.member(user_id),
			Mode::Role(role_id) => self.role(role_id),
		};

		match users {
			Some(users) => {
				for user_id in users {
					tokio::spawn(remove_user(self.bot, self.guild_id, user_id));
				}
			}
			None => event!(Level::WARN, ?self.mode, "unable to cleanup"),
		}
	}

	// TODO: interlink these methods somehow
	// role -> get channels
	// channel -> get members
	// member -> check permission
	fn channel(&self, channel_id: ChannelId) -> Option<Vec<UserId>> {
		let channel = match self.bot.cache.guild_channel(channel_id)? {
			twilight_model::channel::GuildChannel::Voice(c) => c,
			_ => return Some(vec![]),
		};
		if !permission(
			&channel,
			self.guild_id,
			self.bot.id,
			&self.bot.cache,
			Permissions::MOVE_MEMBERS,
		)
		.unwrap_or(false)
		{
			return Some(vec![]);
		}

		let voice_states = self
			.bot
			.cache
			.voice_channel_states(channel.id)
			.unwrap_or_default();
		//let members = members(&self.bot.cache, channel.id).unwrap_or_default();
		let mut users = vec![];
		for voice_state in voice_states {
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
				users.push(voice_state.user_id);
			}
		}
		Some(users)
	}

	fn member(&self, user_id: UserId) -> Option<Vec<UserId>> {
		// is member in a voice channel?
		let channel = self
			.bot
			.cache
			.voice_state(user_id, self.guild_id)?
			.channel_id
			.and_then(|channel_id| self.bot.cache.guild_channel(channel_id))
			.and_then(voice_channel)?;

		// closures are dumb
		let bot = self.bot;

		if !permission(
			&channel,
			self.guild_id,
			bot.id,
			&bot.cache,
			Permissions::MOVE_MEMBERS,
		)
		.unwrap_or(false)
		{
			return Some(vec![]);
		}

		if !permission(
			&channel,
			self.guild_id,
			user_id,
			&bot.cache,
			Permissions::CONNECT,
		)
		.unwrap_or(false)
		{
			Some(vec![user_id])
		} else {
			Some(vec![])
		}
	}

	/// Returns a vector of the users to delete
	// FIXME: cleanup unecessary allocation (vec)
	fn role(&self, role_id: Option<RoleId>) -> Option<Vec<UserId>> {
		let managed_channels = self.managed_channels()?;

		let mut users = vec![];

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
					users.push(voice_state.user_id);
				}
			}
		}
		Some(users)
	}

	/// Get all "managed" [`VoiceChannel`]s in a guild.
	///
	/// Managed means that the bot has [`Permissions::MOVE_MEMBERS`] in it.
	fn managed_channels(&self) -> Option<impl Iterator<Item = VoiceChannel> + '_> {
		Some(
			self.bot
				.cache
				.voice_channels(self.guild_id)?
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
				}),
		)
	}
}

/// Filter [`GuildChannel`] to [`Option<VoiceChannel>`]
pub fn voice_channel(channel: GuildChannel) -> Option<VoiceChannel> {
	match channel {
		GuildChannel::Voice(c) => Some(c),
		_ => None,
	}
}
