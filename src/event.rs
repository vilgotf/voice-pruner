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

#[tracing::instrument(skip(bot), fields(%guild_id, ?scope))]
async fn auto_prune(bot: Bot, guild_id: Id<GuildMarker>, scope: Scope) {
	/// Returns `true` if bot has the "no-auto-prune" role.
	fn is_disabled(bot: Bot, guild_id: Id<GuildMarker>) -> bool {
		if let Some(member) = bot.cache.member(guild_id, bot.id) {
			member.roles().iter().any(|&role_id| {
				bot.cache
					.role(role_id)
					.map_or(false, |role| role.name == "no-auto-prune")
			})
		} else {
			// Ordering isn't guarenteed, GuildCreate might be sent after others.
			true
		}
	}

	if is_disabled(bot, guild_id) {
		return;
	}

	let search = bot.search(guild_id);

	let users = match scope {
		Scope::Channel(channel) => bot
			.is_monitored(channel)
			.then(|| search.channel(channel))
			.unwrap_or_default(),
		Scope::Guild => search.guild(),
		Scope::User(user) => {
			return if search.user(user) {
				bot.remove(guild_id, Some(user)).await;
			}
		}
	};

	bot.remove(guild_id, users.into_iter()).await;
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
		Event::ChannelUpdate(c) => {
			auto_prune(bot, c.guild_id.expect("present"), Scope::Channel(c.id)).await
		}
		Event::MemberUpdate(m) => {
			auto_prune(bot, m.guild_id, Scope::User(m.user.id)).await;
		}
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
