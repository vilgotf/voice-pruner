//! Bot that on channel, member & role updates goes through the relevant voice channels
//! in the guild and removes members lacking connection permission.

#![feature(option_result_contains)]

use std::{env, ffi::OsStr, fs, ops::Deref, path::PathBuf, result::Result as StdResult};

use anyhow::{Context, Result};
use clap::{crate_authors, crate_description, crate_license, crate_name, crate_version, App, Arg};
use futures::{stream::FuturesUnordered, StreamExt};
use interaction::Interaction;
use search::Search;
use tokio::signal::unix::{signal, SignalKind};
use tracing::{event as log, instrument, Level};
use tracing_subscriber::EnvFilter;
use twilight_cache_inmemory::{InMemoryCache, ResourceType};
use twilight_gateway::{cluster::Events, Cluster, EventTypeFlags, Intents};
use twilight_http::{error::Error as HttpError, Client as HttpClient};
use twilight_model::{
	application::interaction::ApplicationCommand,
	channel::{GuildChannel, VoiceChannel},
	guild::Permissions,
	id::{ChannelId, GuildId, UserId},
};

mod commands;
mod event;
mod interaction;
mod response;
mod search;

#[instrument]
/// Get token from systemd credential storage, falling back to env var.
fn token() -> Result<String> {
	log!(Level::INFO, "searching for systemd credential storage");
	let token = if let Some(credential_dir) = env::var_os("CREDENTIALS_DIRECTORY") {
		log!(Level::INFO, "using systemd credential storage");
		let path: PathBuf = [&credential_dir, OsStr::new("token")].iter().collect();
		fs::read_to_string(path)?
	} else {
		log!(Level::WARN, "falling back to `TOKEN` environment variable");
		env::var("TOKEN")?
	}
	.trim_end()
	.to_owned();

	Ok(token)
}

/// Acquires [`Config`] from cmdline using [`clap::App`]
fn conf() -> Result<Config> {
	let matches = App::new(crate_name!())
		.about(crate_description!())
		.author(crate_authors!())
		.license(crate_license!())
		.version(crate_version!())
		.args(&[
			Arg::new("guild-id")
				.about("Modify slash commands in this guild")
				.env("GUILD_ID")
				.long("guild-id")
				.takes_value(true),
			Arg::new("remove-slash-commands")
				.about("Remove slash commands and exits")
				.env("REMOVE_SLASH_COMMANDS")
				.long("remove-slash-commands"),
		])
		.get_matches();

	let guild_id = match matches.value_of_t::<u64>("guild-id") {
		Ok(g) => Some(g.into()),
		Err(e) if e.kind == clap::ErrorKind::ArgumentNotFound => None,
		Err(e) => e.exit(),
	};
	let remove_slash_commands = matches.is_present("remove-slash-commands");

	Ok(Config {
		guild_id,
		remove_slash_commands,
		token: token()?,
	})
}

struct Config {
	guild_id: Option<GuildId>,
	remove_slash_commands: bool,
	token: String,
}

#[instrument]
#[tokio::main]
async fn main() -> Result<()> {
	// prefer RUST_LOG, fallback to "info".
	let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
	tracing_subscriber::fmt().with_env_filter(filter).init();

	let (bot, events) = Bot::new(conf()?).await.context("Startup failed")?;

	tokio::spawn(bot.connect());

	// Listen to sigint (ctrl-c) and sigterm (docker/podman).
	let mut sigint = signal(SignalKind::interrupt())?;
	let mut sigterm = signal(SignalKind::terminate())?;

	tokio::select! {
		_ = bot.process(events) => (),
		_ = sigint.recv() => log!(Level::INFO, "received SIGINT"),
		_ = sigterm.recv() => log!(Level::INFO, "received SIGTERM"),
	};

	log!(Level::INFO, "shutting down");

	bot.shutdown();
	Ok(())
}

/// The bot's components.
pub struct Components {
	pub cache: InMemoryCache,
	pub cluster: Cluster,
	pub http: HttpClient,
	/// User ID of the bot
	pub id: UserId,
}

// Pointer to the address of components
#[derive(Clone, Copy)]
pub struct Bot(&'static Components);

impl Bot {
	/// Creates a [`Bot`] and an [`Events`] stream from [`Config`].
	async fn new(config: Config) -> Result<(Self, Events)> {
		let http = HttpClient::new(config.token.clone());

		let id = http.current_user().exec().await?.model().await?.id;
		http.set_application_id(id.0.into());

		// run before starting cluster
		if config.remove_slash_commands {
			if let Some(guild_id) = config.guild_id {
				log!(Level::INFO, %guild_id, "removing guild slash commands");
				http.set_guild_commands(guild_id, &[])?.exec().await
			} else {
				log!(Level::INFO, "removing global slash commands");
				http.set_global_commands(&[])?.exec().await
			}
			.context("removing slash commands failed")?;

			std::process::exit(0);
		};

		if let Some(guild_id) = config.guild_id {
			log!(Level::INFO, %guild_id, "setting guild slash commands");
			http.set_guild_commands(guild_id, &commands::commands())?
				.exec()
				.await
		} else {
			log!(Level::INFO, "setting global slash commands");
			http.set_global_commands(&commands::commands())?
				.exec()
				.await
		}
		.context("setting slash commands failed")?;

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

		let (cluster, events) = {
			let intents = Intents::GUILDS | Intents::GUILD_MEMBERS | Intents::GUILD_VOICE_STATES;
			let mut events = EventTypeFlags::GUILDS
				| EventTypeFlags::GUILD_MEMBERS
				| EventTypeFlags::INTERACTION_CREATE
				| EventTypeFlags::READY
				| EventTypeFlags::VOICE_STATE_UPDATE;
			events.remove(EventTypeFlags::CHANNEL_PINS_UPDATE);
			Cluster::builder(config.token, intents)
				.event_types(events)
				.build()
				.await?
		};

		Ok((
			// Arc is a slower alternative since a new task is spawned for
			// incomming events (requiring clone).
			Self(Box::leak(Box::new(Components {
				cache,
				cluster,
				http,
				id,
			}))),
			events,
		))
	}

	/// Connects to the Discord gateway.
	async fn connect(self) {
		self.cluster.up().await;
		log!(Level::INFO, "all shards connected");
	}

	const fn interaction(self, command: ApplicationCommand) -> Interaction {
		Interaction::new(self, command)
	}

	/// Returns `true` if the voice channel is monitored.
	fn monitored(self, channel_id: ChannelId) -> bool {
		self.cache
			.permissions()
			.in_channel(self.id, channel_id)
			.log()
			.map(|p| p.contains(Permissions::MOVE_MEMBERS))
			.unwrap_or_default()
	}

	/// Returns `true` if the user is permitted to be in the voice channel.
	fn permitted(self, user_id: UserId, channel_id: ChannelId) -> bool {
		self.cache
			.permissions()
			.in_channel(user_id, channel_id)
			.log()
			.map(|p| p.contains(Permissions::CONNECT))
			.unwrap_or_default()
	}

	/// Spawns a new task for each [`Event`] in the [`Events`] stream and calls [`event::process`] on it.
	///
	/// [`Event`]: twilight_model::gateway::event::Event
	async fn process(self, mut events: Events) {
		log!(Level::INFO, "started main event stream loop");
		while let Some((_, event)) = events.next().await {
			tokio::spawn(event::process(self, event));
		}
		log!(Level::ERROR, "event stream exhausted (shouldn't happen)");
	}

	/// Removes a user from voice channel.
	async fn remove(self, guild_id: GuildId, user_id: UserId) -> StdResult<(), HttpError> {
		log!(Level::INFO, user.id = %user_id, "kicking");
		self.http
			.update_guild_member(guild_id, user_id)
			.channel_id(None)
			.exec()
			.await?;
		Ok(())
	}

	/// Removes users, logging on error.
	async fn remove_mul(self, guild_id: GuildId, users: impl Iterator<Item = UserId>) {
		let mut futures = users
			.map(|user_id| async move { self.remove(guild_id, user_id).await.log() })
			.collect::<FuturesUnordered<_>>();
		while futures.next().await.is_some() {}
	}

	const fn search(self, guild_id: GuildId) -> Search {
		Search::new(self, guild_id)
	}

	fn shutdown(self) {
		self.cluster.down();
	}
}

impl Deref for Bot {
	type Target = Components;

	fn deref(&self) -> &Self::Target {
		self.0
	}
}

trait InMemoryCacheExt {
	/// Returns a [`GuildChannel::Voice`] from a [`ChannelId`].
	fn voice_channel(&self, channel_id: ChannelId) -> Option<VoiceChannel>;
}

impl InMemoryCacheExt for InMemoryCache {
	fn voice_channel(&self, channel_id: ChannelId) -> Option<VoiceChannel> {
		match self.guild_channel(channel_id)? {
			GuildChannel::Voice(c) => Some(c),
			_ => None,
		}
	}
}

trait Log {
	fn log(self) -> Self;
}

impl<T, E: 'static> Log for StdResult<T, E>
where
	E: std::error::Error,
{
	fn log(self) -> Self {
		if let Err(e) = &self {
			log!(Level::ERROR, error = e as &dyn std::error::Error);
		}
		self
	}
}
