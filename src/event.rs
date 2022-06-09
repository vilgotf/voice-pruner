//! Incoming Discord events.

use twilight_gateway::Event;
use twilight_model::{
	application::interaction::Interaction,
	gateway::payload::incoming::{RoleDelete, RoleUpdate},
	id::{
		marker::{ChannelMarker, GuildMarker, UserMarker},
		Id,
	},
};

use crate::Bot;

#[derive(Debug)]
enum Scope {
	Channel(Id<ChannelMarker>),
	Guild,
	User(Id<UserMarker>),
}

#[tracing::instrument(skip(bot), fields(%guild, ?scope))]
async fn auto_prune(bot: Bot, guild: Id<GuildMarker>, scope: Scope) {
	let is_disabled = || {
		// event order isn't guarenteed, so this might not be cached yet
		bot.cache.member(guild, bot.id).map_or(true, |member| {
			member.roles().iter().any(|&role| {
				bot.cache
					.role(role)
					.map_or(false, |role| role.name == "no-auto-prune")
			})
		})
	};

	if is_disabled() {
		return;
	}

	let search = bot.search(guild);

	let users = match scope {
		Scope::Channel(channel) => bot
			.is_monitored(channel)
			.then(|| search.channel(channel))
			.unwrap_or_default(),
		Scope::Guild => search.guild(),
		Scope::User(user) => {
			return if search.user(user) {
				bot.remove(guild, Some(user)).await;
			}
		}
	};

	bot.remove(guild, users.into_iter()).await;
}

/// Process an event.
pub async fn process(bot: Bot, event: Event) {
	let skip = match &event {
		// channel updates requires recheking its connected users, so skip if channel
		// `permission_overwrites` didn't change
		Event::ChannelUpdate(c) => bot.cache.channel(c.id).map_or(false, |cached| {
			cached.permission_overwrites != c.permission_overwrites
		}),
		// role updates requires rechecking all the guilds connected users, so skip if role
		// permissions didn't change
		Event::RoleUpdate(r) => {
			bot.cache.role(r.role.id).map(|r| r.permissions) != Some(r.role.permissions)
		}
		_ => false,
	};

	bot.cache.update(&event);

	if skip {
		return;
	}

	match event {
		Event::ChannelUpdate(c) => auto_prune(bot, c.guild_id.unwrap(), Scope::Channel(c.id)).await,
		Event::MemberUpdate(m) => auto_prune(bot, m.guild_id, Scope::User(m.user.id)).await,
		Event::RoleDelete(RoleDelete { guild_id, .. })
		| Event::RoleUpdate(RoleUpdate { guild_id, .. }) => {
			auto_prune(bot, guild_id, Scope::Guild).await;
		}
		Event::InteractionCreate(i) => match i.0 {
			Interaction::ApplicationCommand(cmd) => crate::commands::run(bot, *cmd).await,
			interaction => tracing::warn!(?interaction, "unhandled"),
		},
		Event::Ready(r) => tracing::info!(guilds = %r.guilds.len(), user = %r.user.name),
		_ => (),
	}
}
