use tracing::{event, instrument, Level};
use twilight_model::{
	guild::Permissions,
	id::{ChannelId, GuildId, RoleId, UserId},
};

use crate::{Bot, InMemoryCacheExt};

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

pub struct Permission {
	bot: &'static Bot,
	guild_id: GuildId,
	mode: Mode,
}

impl Permission {
	/// Permissions required to not be kicked.
	const REQUIRED_PERMISSIONS: Permissions = Permissions::CONNECT;

	pub const fn new(bot: &'static Bot, guild_id: GuildId, mode: Mode) -> Self {
		Self {
			bot,
			guild_id,
			mode,
		}
	}

	#[instrument(skip(self), fields(guild_id = %self.guild_id))]
	pub async fn act(&self) {
		match self.mode {
			Mode::Channel(c) => {
				if let Some(users) = self.channel(c, None) {
					users.for_each(|user_id| {
						tokio::spawn(remove_user(self.bot, self.guild_id, user_id));
					})
				}
			}
			Mode::Member(user_id) => {
				if self.member(user_id).is_some() {
					remove_user(self.bot, self.guild_id, user_id).await
				}
			}
			Mode::Role(role_id) => {
				if let Some(users) = self.role(role_id) {
					users.for_each(|user_id| {
						tokio::spawn(remove_user(self.bot, self.guild_id, user_id));
					})
				}
			}
		}
	}

	/// Returns whether the voice channel is monitored or not.
	///
	/// # Panics
	/// Might panic if not given a voice channel's id.
	fn is_monitored(&self, channel_id: ChannelId) -> bool {
		self.bot
			.cache
			.permissions()
			.in_channel(self.bot.id, channel_id)
			.expect("cache contains the required info")
			.contains(Permissions::MOVE_MEMBERS)
	}

	/// Returns whether the [`UserId`] has the required permissions to remain connected to a
	/// voice channel.
	///
	/// # Panics
	/// Might panic if not given a voice channel's id.
	fn check_member_perm(&self, user_id: UserId, channel_id: ChannelId) -> bool {
		event!(Level::DEBUG, %user_id, "checking user's permission");
		self.bot
			.cache
			.permissions()
			.in_channel(user_id, channel_id)
			.expect("cache contains the required info")
			.contains(Self::REQUIRED_PERMISSIONS)
	}

	// role -> get channels
	// channel -> get members
	// member -> check permission
	/// Returns an iterator over [`UserId`]'s to be removed from some channel.
	/// If given a role only search for users with that role.
	fn channel(
		&self,
		channel_id: ChannelId,
		role_id: Option<RoleId>,
	) -> Option<impl Iterator<Item = UserId> + '_> {
		// is this channel a voice channel
		let channel = self.bot.cache.voice_channel(channel_id)?;

		if !self.is_monitored(channel.id) {
			return None;
		}

		event!(Level::DEBUG, %channel.id, "searching through channel");
		Some(
			self.bot
				.cache
				.voice_channel_states(channel.id)?
				.into_iter()
				.filter(move |state| {
					if let (Some(role_id), Some(member)) = (role_id, state.member.as_ref()) {
						// member.roles doesn't contain everybody role
						role_id == self.guild_id.0.into() || member.roles.contains(&role_id)
					} else {
						true
					}
				})
				.filter_map(move |state| {
					(!self.check_member_perm(state.user_id, channel.id)).then(|| state.user_id)
				}),
		)
	}

	/// Returns a [`UserId`] that's to be removed.
	fn member(&self, user_id: UserId) -> Option<UserId> {
		// is member in a voice channel?
		let channel_id = self
			.bot
			.cache
			.voice_state(user_id, self.guild_id)?
			.channel_id?;

		if !self.is_monitored(channel_id) {
			return None;
		}

		(!self.check_member_perm(user_id, channel_id)).then(|| user_id)
	}

	/// Returns an iterator over [`UserId`] to be removed.
	fn role(&self, role_id: Option<RoleId>) -> Option<impl Iterator<Item = UserId> + '_> {
		let channels = self.bot.cache.guild_channels(self.guild_id)?;

		Some(
			channels
				.into_iter()
				.filter_map(move |channel_id| self.channel(channel_id, role_id))
				.flatten(),
		)
	}
}
