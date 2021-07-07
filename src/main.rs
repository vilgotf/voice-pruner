//! Bot that on channel, member & role updates goes through the relevant voice channels
//! in the guild and removes members lacking connection permission.

#![forbid(unsafe_code)]

use anyhow::{Context, Result};
use clap::{crate_authors, crate_description, crate_license, crate_name, crate_version, App, Arg};
use event::voice_channel;
use tokio::signal::unix::{signal, SignalKind};
use tokio_stream::{Stream, StreamExt};
use tracing::{event as log, Level};
use twilight_cache_inmemory::{InMemoryCache, ResourceType};
use twilight_gateway::{Cluster, Event, EventTypeFlags, Intents};
use twilight_http::Client as HttpClient;
use twilight_model::{
	application::callback::{CallbackData, InteractionResponse},
	channel::VoiceChannel,
	guild::Permissions,
	id::{GuildId, UserId},
};
use twilight_util::permission_calculator::PermissionCalculator;

mod commands;
mod event;

/// Acquires [`Config`] from cmdline using [`clap::App`]
fn conf() -> Config {
	let matches = App::new(crate_name!())
		.about(crate_description!())
		.author(crate_authors!())
		.license(crate_license!())
		.version(crate_version!())
		.arg(
			Arg::new("remove-slash-commands")
				.about("Removes the global slash commands and exits")
				.env("DELETE_SLASH_COMMANDS")
				.long("delete-slash-commands"),
		)
		.arg(
			Arg::new("token")
				.about("The bot's token")
				.env("TOKEN")
				.required(true)
				.takes_value(true),
		)
		.get_matches();
	let remove_slash_commands = matches.is_present("remove-slash-commands");
	let token = matches
		.value_of("token")
		.expect("required argument")
		.to_owned();
	Config {
		remove_slash_commands,
		token,
	}
}

#[tokio::main]
async fn main() -> Result<()> {
	tracing_subscriber::fmt::init();

	let (bot, events) = Bot::new(conf()).await.context("Startup failed")?;

	bot.up().await;

	// Listen to sigint (ctrl-c) and sigterm (docker/podman).
	let mut sigint = signal(SignalKind::interrupt())?;
	let mut sigterm = signal(SignalKind::terminate())?;

	tokio::select! {
		_ = bot.run(events) => (),
		_ = sigint.recv() => log!(Level::INFO, "received SIGINT"),
		_ = sigterm.recv() => log!(Level::INFO, "received SIGTERM"),
	};

	log!(Level::INFO, "shutting down");

	bot.down();
	Ok(())
}

struct Config {
	remove_slash_commands: bool,
	token: String,
}

/// The bot's components.
///
/// The methods on it are only meant to be called from main.
#[non_exhaustive]
pub struct Bot {
	pub cache: InMemoryCache,
	cluster: Cluster,
	pub http: HttpClient,
	pub id: UserId,
}

impl Bot {
	/// Create a [`Bot`] and [`Event`] stream from [`Config`]
	async fn new(config: Config) -> Result<(&'static mut Self, impl Stream<Item = (u64, Event)>)> {
		let cache = {
			let resource_types = ResourceType::CHANNEL
				| ResourceType::GUILD
				| ResourceType::MEMBER
				| ResourceType::ROLE
				| ResourceType::VOICE_STATE;
			InMemoryCache::builder()
				.resource_types(resource_types)
				.build()
		};
		let http = HttpClient::new(&config.token);

		let id = http.current_user().await?.id;
		http.set_application_id(id.0.into());

		// run now before doing any other networking that's part of the default startup
		if config.remove_slash_commands {
			log!(Level::INFO, "removing all slash commands");
			http.set_global_commands(vec![])?.await?;
			std::process::exit(0);
		};

		log!(Level::DEBUG, "setting slash commands");
		http.set_global_commands(commands::commands())?.await?;

		let (cluster, events) = {
			let intents = Intents::GUILDS | Intents::GUILD_MEMBERS | Intents::GUILD_VOICE_STATES;
			let events = EventTypeFlags::CHANNEL_CREATE
				| EventTypeFlags::CHANNEL_DELETE
				| EventTypeFlags::CHANNEL_UPDATE
				| EventTypeFlags::GUILD_CREATE
				| EventTypeFlags::GUILD_DELETE
				| EventTypeFlags::GUILD_UPDATE
				| EventTypeFlags::INTERACTION_CREATE
				| EventTypeFlags::MEMBER_UPDATE
				| EventTypeFlags::ROLE_CREATE
				| EventTypeFlags::ROLE_DELETE
				| EventTypeFlags::ROLE_UPDATE
				| EventTypeFlags::VOICE_STATE_UPDATE;
			Cluster::builder(&config.token, intents)
				.event_types(events)
				.http_client(http.clone())
				.build()
				.await?
		};
		let bot = Box::leak(Box::new(Self {
			cache,
			cluster,
			http,
			id,
		}));
		Ok((bot, events))
	}

	/// Asynchronously bring the bot up in its own task.
	async fn up(&'static self) {
		log!(Level::INFO, "bringing up bot");
		tokio::spawn(async move {
			self.cluster.up().await;
			log!(Level::INFO, "finished bringing up bot");
		});
	}

	/// Process an event stream using [`event::Handler::process`].
	async fn run(&'static self, mut events: impl Stream<Item = (u64, Event)> + Unpin) {
		log!(Level::INFO, "started main event stream loop");
		while let Some((_, event)) = events.next().await {
			let handler = event::Handler::new(self, event);
			tokio::spawn(handler.process());
		}
		log!(Level::ERROR, "event stream exhausted");
	}

	fn down(&self) {
		self.cluster.down();
	}
}

trait InMemoryCacheExt {
	/// Get all cached [`VoiceChannel`]s in a guild.
	///
	/// This is a O(m * 2) operation:
	///
	/// * [`guild_channels`] --- get the Guild's [`ChannelId`]s'
	/// * [`guild_channel`] on every [`ChannelId`] --- filter non [`VoiceChannel`]s
	///
	/// Requires the [`Guilds`] intent.
	///
	/// [`ChannelId`]: twilight_model::id::ChannelId
	/// [`Guilds`]: twilight_model::gateway::Intents::GUILDS
	/// [`guild_channels`]: InMemoryCache::guild_channels
	/// [`guild_channel`]: InMemoryCache::guild_channel

	// Cache uses HashSet internally for fast updates, Vec is better here since it's short lived.
	fn voice_channels(&self, guild_id: GuildId) -> Option<Vec<VoiceChannel>>;
}

impl InMemoryCacheExt for InMemoryCache {
	fn voice_channels(&self, guild_id: GuildId) -> Option<Vec<VoiceChannel>> {
		let channels = self
			.guild_channels(guild_id)?
			.into_iter()
			.filter_map(|channel_id| voice_channel(self.guild_channel(channel_id).unwrap()))
			.collect();
		Some(channels)
	}
}

/// Returns wether user has some permission in the given [`VoiceChannel`].
// TODO: replace with cache built in permission calc
fn permission(
	channel: &VoiceChannel,
	guild_id: GuildId,
	user_id: UserId,
	cache: &InMemoryCache,
	permission: Permissions,
) -> Option<bool> {
	let member_roles = cache
		.member(guild_id, user_id)?
		.roles
		.into_iter()
		.map(|role_id| {
			(
				role_id,
				cache.role(role_id).expect("valid role_id").permissions,
			)
		})
		.collect::<Vec<_>>();
	let everyone_role = cache
		.role(guild_id.0.into())
		.expect("valid role_id")
		.permissions;
	let calc = PermissionCalculator::new(guild_id, user_id, everyone_role, &member_roles);

	Some(
		calc.in_channel(channel.kind, &channel.permission_overwrites)
			.contains(permission),
	)
}

fn response(msg: impl Into<String>) -> InteractionResponse {
	let msg = msg.into();
	if msg.is_empty() {
		panic!("empty message is not allowed")
	}

	InteractionResponse::ChannelMessageWithSource(CallbackData {
		allowed_mentions: None,
		content: Some(msg),
		embeds: Vec::new(),
		flags: None,
		tts: None,
	})
}
