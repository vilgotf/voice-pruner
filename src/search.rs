use anyhow::anyhow;
use const_format::formatcp;
use tracing::{event, Level};
use twilight_model::{
	id::{
		marker::{ChannelMarker, GuildMarker, RoleMarker, UserMarker},
		Id,
	},
	voice::VoiceState,
};

use crate::{Permissions, Symbol};

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
				Some(formatcp!("{} **Not a voice channel**", Symbol::WARNING))
			}
			Error::NotInVoice => Some(formatcp!(
				"{} **User is not in a voice channel**",
				Symbol::WARNING
			)),
			Error::Unmonitored => Some("**Channel is unmonitored**"),
		}
	}
}

/// Search for users in voice channels that should be removed.
#[derive(Clone, Copy)]
pub struct Search {
	pub(super) bot: Bot,
	pub(super) guild_id: Id<GuildMarker>,
}

impl Search {
	/// Returns `true` if the user is permitted to be in the voice channel.
	fn is_permitted(&self, state: &VoiceState) -> bool {
		let channel_id = state.channel_id.expect("included since it's from cache");

		self.bot
			.cache
			.permissions()
			.in_channel(state.user_id, channel_id)
			.expect("resources are available")
			.contains(Permissions::CONNECT)
	}

	/// Returns a list of [`Id<UserMarker>`]'s to be removed.
	///
	/// If given a role only search users with that role.
	pub fn channel(
		self,
		channel_id: Id<ChannelMarker>,
		role_id: Option<Id<RoleMarker>>,
	) -> Result<Vec<Id<UserMarker>>, Error> {
		// is this channel a voice channel
		if !matches!(
			self.bot
				.cache
				.guild_channel(channel_id)
				.ok_or_else(|| Error::Internal(anyhow!("channel not in cache")))?
				.kind(),
			crate::MONITORED_CHANNEL_TYPES
		) {
			return Err(Error::NotAVoiceChannel);
		}

		if !self.bot.is_monitored(channel_id) {
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
					role_id == self.guild_id.cast() || member.roles.contains(&role_id)
				} else {
					true
				}
			})
			.filter_map(|state| (!self.is_permitted(&state)).then(|| state.user_id))
			.collect())
	}

	/// Returns a list of [`Id<UserMarker>`]'s to be removed.
	///
	/// If given a role only search users with that role.
	pub fn guild(self, role_id: Option<Id<RoleMarker>>) -> Result<Vec<Id<UserMarker>>, Error> {
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

	/// Returns `true` if a [`Id<UserMarker>`] should be removed.
	pub fn user(self, user_id: Id<UserMarker>) -> Result<bool, Error> {
		// is member in a voice channel?
		let state = self
			.bot
			.cache
			.voice_state(user_id, self.guild_id)
			.ok_or(Error::NotInVoice)?;

		if !self
			.bot
			.is_monitored(state.channel_id.expect("always set in cache"))
		{
			return Err(Error::Unmonitored);
		}

		Ok(!self.is_permitted(&state))
	}
}
