use anyhow::anyhow;
use tracing::{event, Level};
use twilight_model::{
	channel::GuildChannel,
	id::{ChannelId, GuildId, RoleId, UserId},
};

use crate::response::Emoji;

use super::Bot;

pub enum Error {
	Internal(anyhow::Error),
	NotAVoiceChannel,
	NotInVoice,
	Unmonitored,
}

impl Error {
	pub fn msg(self) -> Option<String> {
		match self {
			Error::Internal(e) => {
				event!(Level::ERROR, error = &*e as &dyn std::error::Error);
				None
			}
			Error::NotAVoiceChannel => Some(format!("{} **Not a voice channel**", Emoji::WARNING)),
			Error::NotInVoice => Some(format!(
				"{} **User is not in a voice channel**",
				Emoji::WARNING
			)),
			Error::Unmonitored => Some(String::from("**Channel is unmonitored**")),
		}
	}
}

/// Search for users in voice channels that should be removed.
#[derive(Clone, Copy)]
pub struct Search {
	bot: Bot,
	guild_id: GuildId,
}

impl Search {
	pub const fn new(bot: Bot, guild_id: GuildId) -> Self {
		Self { bot, guild_id }
	}

	/// Returns an iterator over [`UserId`]'s to be removed.
	/// If given a role only search users with that role.
	pub fn channel(
		self,
		channel_id: ChannelId,
		role_id: Option<RoleId>,
	) -> Result<impl Iterator<Item = UserId>, Error> {
		// is this channel a voice channel
		match self
			.bot
			.cache
			.guild_channel(channel_id)
			.ok_or_else(|| Error::Internal(anyhow!("channel not in cache")))?
		{
			GuildChannel::Voice(_) => (),
			_ => return Err(Error::NotAVoiceChannel),
		};

		if !self.bot.monitored(channel_id) {
			return Err(Error::Unmonitored);
		}

		event!(Level::DEBUG, %channel_id, "searching through channel");
		Ok(self
			.bot
			.cache
			.voice_channel_states(channel_id)
			.unwrap_or_default()
			.into_iter()
			.filter(move |state| {
				if let (Some(role_id), Some(member)) = (role_id, state.member.as_ref()) {
					// member.roles doesn't contain everybody role
					role_id == self.guild_id.0.into() || member.roles.contains(&role_id)
				} else {
					true
				}
			})
			.filter_map(move |state| (!self.bot.permitted(&state)).then(|| state.user_id)))
	}

	/// Returns an iterator over [`UserId`]'s to be removed.
	/// If given a role only search users with that role.
	pub fn guild(self, role_id: Option<RoleId>) -> Result<impl Iterator<Item = UserId>, Error> {
		let channels = self
			.bot
			.cache
			.guild_channels(self.guild_id)
			.ok_or_else(|| Error::Internal(anyhow!("guild not in cache")))?;

		Ok(channels
			.into_iter()
			.filter_map(move |channel_id| self.channel(channel_id, role_id).ok())
			.flatten())
	}

	/// Returns `true` if a [`UserId`] should be removed.
	pub fn user(self, user_id: UserId) -> Result<bool, Error> {
		// is member in a voice channel?
		let state = self
			.bot
			.cache
			.voice_state(user_id, self.guild_id)
			.ok_or(Error::NotInVoice)?;

		if !self
			.bot
			.monitored(state.channel_id.expect("always set in cache"))
		{
			return Err(Error::Unmonitored);
		}

		Ok(!self.bot.permitted(&state))
	}
}
