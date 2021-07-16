//! Handles events form Discord

use permission::{Mode, Permission};
use tracing::{event, Level};
use twilight_gateway::Event;
use twilight_model::{
	application::interaction::Interaction,
	channel::{Channel, GuildChannel},
};

use crate::{Bot, InMemoryCacheExt};

// For PartialApplicationCommand
pub mod command;
// For voice_channel
pub mod permission;

/// Match an [`Event`] and execute it.
pub async fn process(bot: &'static Bot, event: Event) {
	let skip = match &event {
		// don't panic here
		Event::ChannelUpdate(c) => match &c.0 {
			Channel::Guild(c) => match c {
				GuildChannel::Voice(v) => bot
					.cache
					.voice_channel(c.id())
					.map(|vc| vc.permission_overwrites == v.permission_overwrites)
					.unwrap_or_default(),
				_ => true,
			},
			_ => true,
		},
		Event::MemberUpdate(m) => bot
			.cache
			.member(m.guild_id, m.user.id)
			.map(|cm| m.roles == cm.roles)
			.unwrap_or_default(),
		Event::RoleUpdate(r) => bot
			.cache
			.role(r.role.id)
			.map(|cr| cr.permissions == r.role.permissions)
			.unwrap_or_default(),
		_ => false,
	};

	bot.cache.update(&event);

	if skip {
		event!(Level::DEBUG, "skipping event");
		return;
	}

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
