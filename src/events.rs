//! Handles events form Discord

use permission::{Mode, Permission};
use tracing::{event, instrument, Level};
use twilight_gateway::Event;
use twilight_model::{
	application::interaction::{ApplicationCommand, Interaction},
	channel::{Channel, GuildChannel},
	gateway::payload::incoming::ChannelUpdate,
};

use crate::{command::Command, Bot, InMemoryCacheExt};

mod permission;

/// Handles an [`Event`].
pub async fn process(bot: Bot, event: Event) {
	bot.cache.update(&event);

	match event {
		Event::ChannelUpdate(ChannelUpdate(Channel::Guild(GuildChannel::Voice(vc))))
			// channel updates requires recheking its connected users, so skip if channel
			// `permission_overwrites` didn't change
			if bot.cache.voice_channel(vc.id).map(|vc| vc.permission_overwrites).as_ref()
			!= Some(&vc.permission_overwrites) =>
		{
			if let Some(guild_id) = vc.guild_id {
				Permission::new(bot, guild_id, Mode::Channel(vc.id))
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
		Event::RoleUpdate(r)
			// role updates requires rechecking all the guilds connected users, so skip if role
			// permissions didn't change
			if bot.cache.role(r.role.id).map(|r| r.permissions) != Some(r.role.permissions) =>
		{
			Permission::new(bot, r.guild_id, Mode::Role(Some(r.role.id)))
				.act()
				.await;
		}
		Event::InteractionCreate(i) => match i.0 {
			Interaction::ApplicationCommand(cmd) => command(bot, *cmd).await,
			interaction => event!(Level::WARN, ?interaction, "unhandled"),
		},
		Event::Ready(r) => event!(Level::INFO, guilds = %r.guilds.len(), user = %r.user.name),
		_ => (),
	}
}

#[instrument(skip(bot, cmd), fields(guild_id, command.name = %cmd.data.name))]
async fn command(bot: Bot, cmd: ApplicationCommand) {
	if let Some(cmd) = Command::get(cmd) {
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
