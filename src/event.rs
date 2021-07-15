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

/// Match an [`Event`] and execute it.
pub async fn process(bot: &'static Bot, event: Event) {
	bot.cache.update(&event);

	// TODO: only run on permission updates instead of on all updates.
	match event {
		Event::ChannelUpdate(c) => {
			let (channel_id, guild_id) = match c.0 {
				Channel::Guild(c) => (c.id(), c.guild_id().expect("?? is always a guild")),
				_ => return,
			};

			Permission::new(bot, guild_id, Mode::Channel(channel_id))
				.act()
				.await;
		}

		Event::MemberUpdate(m) => {
			Permission::new(bot, m.guild_id, Mode::Member(m.user.id))
				.act()
				.await
		}

		Event::RoleDelete(r) => {
			Permission::new(bot, r.guild_id, Mode::Role(None))
				.act()
				.await
		}

		Event::RoleUpdate(r) => {
			Permission::new(bot, r.guild_id, Mode::Role(Some(r.role.id)))
				.act()
				.await
		}

		Event::InteractionCreate(slash_interaction) => match slash_interaction.0 {
			Interaction::Ping(_) => (),
			Interaction::ApplicationCommand(command) => command::act(bot, *command).await,
			i => event!(Level::WARN, ?i, "unhandled interaction"),
		},

		Event::Ready(ready) => {
			event!(Level::INFO, user_name = %ready.user.name);
			event!(Level::INFO, guilds = %ready.guilds.len());
		}
		_ => (),
	}
}
