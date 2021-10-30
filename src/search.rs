use anyhow::anyhow;
use const_format::formatcp;
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
	/// Returns the error message unless it's internal (then it's logged).
	pub fn msg(self) -> Option<&'static str> {
		match self {
			Error::Internal(e) => {
				event!(Level::ERROR, error = &*e as &dyn std::error::Error);
				None
			}
			Error::NotAVoiceChannel => {
				Some(formatcp!("{} **Not a voice channel**", Emoji::WARNING))
			}
			Error::NotInVoice => Some(formatcp!(
				"{} **User is not in a voice channel**",
				Emoji::WARNING
			)),
			Error::Unmonitored => Some("**Channel is unmonitored**"),
		}
	}
}

/// Search for users in voice channels that should be removed.
#[derive(Clone, Copy)]
pub struct Search {
	pub(super) bot: Bot,
	pub(super) guild_id: GuildId,
}

impl Search {
	/// Returns a list of [`UserId`]'s to be removed.
	///
	/// If given a role only search users with that role.
	pub fn channel(
		self,
		channel_id: ChannelId,
		role_id: Option<RoleId>,
	) -> Result<Vec<UserId>, Error> {
		// is this channel a voice channel
		if !matches!(
			self.bot
				.cache
				.guild_channel(channel_id)
				.ok_or_else(|| Error::Internal(anyhow!("channel not in cache")))?
				.value()
				.resource(),
			GuildChannel::Voice(_)
		) {
			return Err(Error::NotAVoiceChannel);
		}

		if !self.bot.monitored(channel_id) {
			return Err(Error::Unmonitored);
		}

		event!(Level::DEBUG, %channel_id, "searching through channel");
		Ok(self
			.bot
			.cache
			.voice_channel_states(channel_id)
			.into_iter()
			.flatten()
			.filter(|state| {
				if let (Some(role_id), Some(member)) = (role_id, state.member.as_ref()) {
					// member.roles doesn't contain everybody role
					role_id == self.guild_id.0.into() || member.roles.contains(&role_id)
				} else {
					true
				}
			})
			.filter_map(|state| (!self.bot.permitted(&state)).then(|| state.user_id))
			.collect())
	}

	/// Returns a list of [`UserId`]'s to be removed.
	///
	/// If given a role only search users with that role.
	pub fn guild(self, role_id: Option<RoleId>) -> Result<Vec<UserId>, Error> {
		let channels = self
			.bot
			.cache
			.guild_channels(self.guild_id)
			.ok_or_else(|| Error::Internal(anyhow!("guild not in cache")))?;

		Ok(channels
			.iter()
			.filter_map(|&channel_id| self.channel(channel_id, role_id).ok())
			.flatten()
			.collect())
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
