//! Logic for permission updates (auto pruning).

use std::iter::once;

use tracing::instrument;
use twilight_model::id::{ChannelId, GuildId, RoleId, UserId};

use crate::Bot;

#[derive(Debug)]
pub enum Mode {
	Channel(ChannelId),
	Member(UserId),
	Role(Option<RoleId>),
}

pub struct Permission {
	bot: Bot,
	guild_id: GuildId,
	mode: Mode,
}

impl Permission {
	pub const fn new(bot: Bot, guild_id: GuildId, mode: Mode) -> Self {
		Self {
			bot,
			guild_id,
			mode,
		}
	}

	/// Returns `true` if bot has the "no-auto-prune" role.
	fn is_disabled(&self) -> bool {
		self.bot
			.cache
			.member(self.guild_id, self.bot.id)
			.expect("cache contains bot")
			.roles
			.into_iter()
			.any(|role_id| {
				self.bot
					.cache
					.role(role_id)
					.map(|role| role.name == "no-auto-prune")
					.unwrap_or_default()
			})
	}

	#[instrument(name = "auto-prune", skip(self), fields(%self.guild_id, ?self.mode))]
	pub async fn act(&self) {
		if self.is_disabled() {
			return;
		}

		let search = self.bot.search(self.guild_id);

		match self.mode {
			Mode::Channel(c) => {
				if let Ok(users) = search.channel(c, None) {
					self.bot.remove(self.guild_id, users).await;
				}
			}
			Mode::Member(user_id) => {
				if search.user(user_id).unwrap_or_default() {
					self.bot.remove(self.guild_id, once(user_id)).await;
				}
			}
			Mode::Role(role_id) => {
				if let Ok(users) = search.guild(role_id) {
					self.bot.remove(self.guild_id, users).await;
				}
			}
		}
	}
}