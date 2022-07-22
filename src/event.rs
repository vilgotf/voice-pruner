//! Incoming Discord events.

use twilight_gateway::Event;
use twilight_model::{
	application::interaction::InteractionType,
	gateway::payload::incoming::{RoleDelete, RoleUpdate},
	id::{
		marker::{ChannelMarker, GuildMarker, UserMarker},
		Id,
	},
};

use crate::{BOT, MONITORED_CHANNEL_TYPES};

#[derive(Debug)]
enum Scope {
	Channel(Id<ChannelMarker>),
	Guild,
	User(Id<UserMarker>),
}

#[tracing::instrument(fields(%guild, ?scope))]
async fn auto_prune(guild: Id<GuildMarker>, scope: Scope) {
	let is_disabled = || {
		// event order isn't guarenteed, so this might not be cached yet
		BOT.cache.member(guild, BOT.id).map_or(true, |member| {
			member.roles().iter().any(|&role| {
				BOT.cache
					.role(role)
					.map_or(false, |role| role.name == "no-auto-prune")
			})
		})
	};

	if is_disabled() {
		return;
	}

	let search = BOT.search(guild);

	let users = match scope {
		Scope::Channel(channel) => BOT
			.is_monitored(channel)
			.then(|| search.channel(channel))
			.unwrap_or_default(),
		Scope::Guild => search.guild(),
		Scope::User(user) => {
			return if search.user(user) {
				BOT.remove(guild, Some(user)).await;
			}
		}
	};

	BOT.remove(guild, users.into_iter()).await;
}

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
		Event::ChannelUpdate(c) => auto_prune(c.guild_id.unwrap(), Scope::Channel(c.id)).await,
		Event::MemberUpdate(m) => auto_prune(m.guild_id, Scope::User(m.user.id)).await,
		Event::RoleDelete(RoleDelete { guild_id, .. })
		| Event::RoleUpdate(RoleUpdate { guild_id, .. }) => {
			auto_prune(guild_id, Scope::Guild).await;
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
