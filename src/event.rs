//! Incoming Discord events.

use twilight_gateway::Event;
use twilight_model::{
	application::interaction::InteractionType,
	gateway::payload::incoming::{RoleDelete, RoleUpdate},
};

use crate::{BOT, MONITORED_CHANNEL_TYPES};

/// Process an event.
pub async fn process(event: Event) {
	let skip = match &event {
		// skip if ChannelType is not monitored OR `permission_overwrites` did not change
		Event::ChannelUpdate(c) => {
			!MONITORED_CHANNEL_TYPES.contains(&c.kind)
				|| BOT.cache.channel(c.id).map_or(false, |cached| {
					cached.permission_overwrites != c.permission_overwrites
				})
		}
		// skip if permissions did not change
		Event::RoleUpdate(r) => {
			BOT.cache.role(r.role.id).map(|r| r.permissions) != Some(r.role.permissions)
		}
		_ => false,
	};

	BOT.cache.update(&event);

	if skip {
		return;
	}

	match event {
		Event::ChannelUpdate(c) if BOT.auto_prune(c.guild_id.unwrap()) => {
			crate::prune::channel(c.id, c.guild_id.unwrap()).await;
		}
		Event::MemberUpdate(m) => {
			if BOT.auto_prune(m.guild_id) {
				crate::prune::user(m.guild_id, m.user.id).await;
			}
		}
		Event::RoleDelete(RoleDelete { guild_id, .. })
		| Event::RoleUpdate(RoleUpdate { guild_id, .. })
			if BOT.auto_prune(guild_id) =>
		{
			crate::prune::guild(guild_id).await;
		}
		Event::InteractionCreate(interaction) => match interaction.kind {
			InteractionType::ApplicationCommand => {
				crate::commands::interaction(interaction.0).await
			}
			_ => tracing::warn!(?interaction, "unhandled"),
		},
		Event::Ready(r) => tracing::info!(guilds = %r.guilds.len(), user = %r.user.name),
		_ => (),
	}
}
