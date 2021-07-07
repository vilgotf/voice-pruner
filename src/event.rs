//! Handles events form Discord

use tracing::{event, Level};
use twilight_gateway::Event;
use twilight_model::{
	application::interaction::Interaction,
	channel::{Channel, GuildChannel, VoiceChannel},
	guild::Permissions,
	id::{GuildId, UserId},
};

use crate::{permission, Bot};

mod channel;
mod interaction;
mod member;
mod role;

/// See [`Handler::process`].
pub struct Handler {
	bot: &'static Bot,
	event: Event,
}

impl Handler {
	/// Create a [`Handler`].
	pub const fn new(bot: &'static Bot, event: Event) -> Self {
		Self { bot, event }
	}

	/// Match an [`Event`] and execute it.
	pub async fn process(self) {
		self.bot.cache.update(&self.event);

		// TODO: optimization, only run if permission changed
		match self.event {
			// 1. check if guild channel.
			// 2. check if voice channel
			// 3. check for move permission - wait upstreaming permission into cache
			// 4. get users in channel
			// 5. check em
			Event::ChannelUpdate(c) => {
				let channel = match c.0 {
					Channel::Guild(c) => voice_channel(c),
					_ => return,
				};

				if let Some(channel) = channel {
					let guild_id = channel.guild_id.unwrap();
					if !permission(
						&channel,
						guild_id,
						self.bot.id,
						&self.bot.cache,
						Permissions::MOVE_MEMBERS,
					)
					.unwrap_or(false)
					{
						return;
					}

					let voice_states = self
						.bot
						.cache
						.voice_channel_states(channel.id)
						.unwrap_or_default();
					//let members = members(&self.bot.cache, channel.id).unwrap_or_default();
					for voice_state in voice_states {
						event!(Level::DEBUG, %voice_state.user_id, "checking user");
						if !permission(
							&channel,
							guild_id,
							voice_state.user_id,
							&self.bot.cache,
							Permissions::CONNECT,
						)
						.unwrap_or(true)
						{
							remove_user(self.bot, guild_id, voice_state.user_id).await
						}
					}
				}
			}

			// 1. check user if in voice channel
			// 2. check for move permission - wait upstreaming permission into cache
			// 3. check user permission
			Event::MemberUpdate(m) => {
				// closures are dumb
				let bot = self.bot;

				if let Some(state) = self.bot.cache.voice_state(m.user.id, m.guild_id) {
					// do this for permission argument requirement
					let channel = state
						.channel_id
						.and_then(|channel_id| bot.cache.guild_channel(channel_id))
						.and_then(voice_channel)
						.expect("valid channel");

					if !permission(
						&channel,
						m.guild_id,
						bot.id,
						&bot.cache,
						Permissions::MOVE_MEMBERS,
					)
					.unwrap_or(false)
					{
						return;
					}

					if !permission(
						&channel,
						m.guild_id,
						m.user.id,
						&bot.cache,
						Permissions::CONNECT,
					)
					.unwrap_or(false)
					{
						remove_user(bot, m.guild_id, m.user.id).await
					}
				}
			}

			Event::RoleDelete(r) => {
				let handler = role::Handler::new(self.bot, r.guild_id);
				handler.process(None).await;
			}

			Event::RoleUpdate(r) => {
				let handler = role::Handler::new(self.bot, r.guild_id);
				handler.process(Some(r.role.id)).await;
			}

			Event::InteractionCreate(slash_interaction) => match slash_interaction.0 {
				Interaction::Ping(_) => (),
				Interaction::ApplicationCommand(command) => {
					let handler = interaction::Handler::new(self.bot);
					handler.process(*command).await;
				}
				i => event!(Level::WARN, ?i, "unhandled interaction"),
			},

			Event::Ready(ready) => {
				event!(Level::INFO, user_name = %ready.user.name);
				event!(Level::INFO, guilds = %ready.guilds.len());
			}
			_ => (),
		}
	}
}

/// Tries to remove a user from voice channel in a new task, logging on error.
async fn remove_user(bot: &'static Bot, guild_id: GuildId, user_id: UserId) {
	event!(Level::INFO, member.user.id = %user_id, "kicking user");
	tokio::spawn(async move {
		if let Err(e) = bot
			.http
			.update_guild_member(guild_id, user_id)
			.channel_id(None)
			.await
		{
			// Workaround tracing::Value being weird tokio-rs/tracing#1308
			event!(Level::ERROR, error = &e as &dyn std::error::Error);
		}
	});
}

/// Filter [`GuildChannel`] to [`Option<VoiceChannel>`]
pub fn voice_channel(channel: GuildChannel) -> Option<VoiceChannel> {
	match channel {
		GuildChannel::Voice(c) => Some(c),
		_ => None,
	}
}
