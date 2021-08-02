//! Handles events form Discord

use permission::{Mode, Permission};
use tracing::{event, instrument, Level};
use twilight_gateway::Event;
use twilight_model::{
	application::interaction::{ApplicationCommand, Interaction},
	channel::{Channel, GuildChannel},
};

use crate::{commands::Commands, Bot, InMemoryCacheExt};

mod permission;

/// Handles an [`Event`].
pub async fn process(bot: Bot, event: Event) {
	let skip = match &event {
		Event::ChannelUpdate(c) => match &c.0 {
			Channel::Guild(c) => match c {
				GuildChannel::Voice(vc) => {
					bot.cache
						.voice_channel(c.id())
						.map(|vc| vc.permission_overwrites)
						.as_ref() == Some(&vc.permission_overwrites)
				}
				_ => true,
			},
			_ => true,
		},
		Event::RoleUpdate(r) => bot
			.cache
			.role(r.role.id)
			.map(|r| r.permissions)
			.eq(&Some(r.role.permissions)),
		_ => false,
	};

	bot.cache.update(&event);

	if skip {
		event!(Level::DEBUG, ?event, "skipping event");
		return;
	}

	match event {
		Event::ChannelUpdate(c) => {
			if let (Some(guild_id), id) = match c.0 {
				Channel::Guild(c) => (c.guild_id(), c.id()),
				_ => return,
			} {
				Permission::new(bot, guild_id, Mode::Channel(id))
					.act()
					.await;
			} else {
				event!(Level::WARN, "guild ID missing from ChannelUpdate event");
			}
		}

		Event::MemberUpdate(m) => {
			Permission::new(bot, m.guild_id, Mode::Member(m.user.id))
				.act()
				.await;
		}

		Event::RoleDelete(r) => {
			Permission::new(bot, r.guild_id, Mode::Role(None))
				.act()
				.await;
		}

		Event::RoleUpdate(r) => {
			Permission::new(bot, r.guild_id, Mode::Role(Some(r.role.id)))
				.act()
				.await;
		}

		Event::InteractionCreate(slash_interaction) => match slash_interaction.0 {
			Interaction::ApplicationCommand(cmd) => command(bot, *cmd).await,
			i => event!(Level::WARN, ?i, "unhandled interaction"),
		},

		Event::Ready(r) => event!(Level::INFO, guilds = %r.guilds.len(), user = %r.user.name),

		_ => (),
	}
}

#[instrument(skip(bot, cmd), fields(guild_id, command.name = %cmd.data.name))]
async fn command(bot: Bot, cmd: ApplicationCommand) {
	if let Some(cmd) = Commands::r#match(cmd) {
		if let Err(e) = cmd.run(bot).await {
			event!(
				Level::ERROR,
				error = &*e as &dyn std::error::Error,
				"error running command"
			);
		}
	} else {
		event!(Level::WARN, "received unregistered command");
	}
}
