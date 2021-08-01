//! Handles events form Discord

use permission::{Mode, Permission};
use tracing::{event, Level};
use twilight_gateway::Event;
use twilight_model::{
	application::interaction::Interaction,
	channel::{Channel, GuildChannel},
};

use crate::{Bot, InMemoryCacheExt};

mod command;
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
		Event::MemberUpdate(m) => {
			bot.cache
				.member(m.guild_id, m.user.id)
				.map(|m| m.roles)
				.as_ref() == Some(&m.roles)
		}
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
			Interaction::ApplicationCommand(command) => command::act(bot, *command).await,
			i => event!(Level::WARN, ?i, "unhandled interaction"),
		},

		_ => (),
	}
}
