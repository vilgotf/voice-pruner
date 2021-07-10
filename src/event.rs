//! Handles events form Discord

use permission::{Mode, Permission};
use tracing::{event, Level};
use twilight_gateway::Event;
use twilight_model::{application::interaction::Interaction, channel::Channel};

use crate::Bot;

// For PartialApplicationCommand
pub mod command;
// For voice_channel
pub mod permission;

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
				let (channel_id, guild_id) = match c.0 {
					Channel::Guild(c) => (c.id(), c.guild_id().expect("?? is always a guild")),
					_ => return,
				};

				Permission::new(self.bot, guild_id, Mode::Channel(channel_id))
					.act()
					.await;
			}

			// 1. check user if in voice channel
			// 2. check for move permission - wait upstreaming permission into cache
			// 3. check user permission
			Event::MemberUpdate(m) => {
				Permission::new(self.bot, m.guild_id, Mode::Member(m.user.id))
					.act()
					.await
			}

			Event::RoleDelete(r) => {
				Permission::new(self.bot, r.guild_id, Mode::Role(None))
					.act()
					.await
			}

			Event::RoleUpdate(r) => {
				Permission::new(self.bot, r.guild_id, Mode::Role(Some(r.role.id)))
					.act()
					.await
			}

			Event::InteractionCreate(slash_interaction) => match slash_interaction.0 {
				Interaction::Ping(_) => (),
				Interaction::ApplicationCommand(command) => command::act(self.bot, *command).await,
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
