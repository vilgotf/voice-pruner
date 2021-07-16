//! Bot that on channel, member & role updates goes through the relevant voice channels
//! in the guild and removes members lacking connection permission.

#![deny(clippy::inconsistent_struct_constructor)]
#![deny(rustdoc::broken_intra_doc_links)]
#![forbid(unsafe_code)]
#![warn(clippy::cargo, clippy::nursery, clippy::pedantic)]

use std::{env, ffi::OsStr, fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::{crate_authors, crate_description, crate_license, crate_name, crate_version, App, Arg};
use tokio::signal::unix::{signal, SignalKind};
use tokio_stream::StreamExt;
use tracing::{event as log, instrument, Level};
use twilight_cache_inmemory::{InMemoryCache, ResourceType};
use twilight_gateway::{cluster::Events, Cluster, EventTypeFlags, Intents};
use twilight_http::Client as HttpClient;
use twilight_model::{
	channel::{GuildChannel, VoiceChannel},
	id::{ChannelId, GuildId, UserId},
};

pub use event::command::PartialApplicationCommand;

mod commands;
mod event;
mod response;

#[instrument]
/// Get token from systemd credential storage, falling back to env var.
fn token() -> Result<String> {
	let token = if let Some(credential_dir) = env::var_os("CREDENTIALS_DIRECTORY") {
		log!(Level::INFO, "using systemd credential storage");
		let path: PathBuf = [&credential_dir, OsStr::new("token")].iter().collect();
		fs::read_to_string(path)?
	} else {
		log!(Level::WARN, "falling back to `TOKEN` environment variable");
		env::var("TOKEN")?
	};

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
				.about("Don't add / remove slash commands globaly but instead just in this guild")
				.env("GUILD_ID")
				.long("guild-id")
				.takes_value(true),
			Arg::new("remove-slash-commands")
				.about("Removes the global slash commands and exits")
				.env("DELETE_SLASH_COMMANDS")
				.long("delete-slash-commands"),
		])
		.get_matches();
	let guild_id = match matches.value_of_t::<u64>("guild-id") {
		Ok(g) => Some(g.into()),
		Err(e) if e.kind == clap::ErrorKind::ArgumentNotFound => None,
		Err(e) => e.exit(),
	};
	let remove_slash_commands = matches.is_present("remove-slash-commands");
	let token = token()?;
	Ok(Config {
		guild_id,
		remove_slash_commands,
		token,
	})
}

struct Config {
	guild_id: Option<GuildId>,
	remove_slash_commands: bool,
	token: String,
}

#[tokio::main]
async fn main() -> Result<()> {
	tracing_subscriber::fmt::init();

	let (bot, events) = Bot::new(conf()?).await.context("Startup failed")?;

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

/// The bot's components.
///
/// The methods on it are only meant to be called from main.
pub struct Bot {
	pub cache: InMemoryCache,
	cluster: Cluster,
	pub http: HttpClient,
	pub id: UserId,
}

impl Bot {
	/// Create a [`Bot`] and [`Events`] stream from [`Config`]
	async fn new(config: Config) -> Result<(&'static mut Self, Events)> {
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
			if let Some(guild_id) = config.guild_id {
				http.set_guild_commands(guild_id, vec![])?.await
			} else {
				http.set_global_commands(vec![])?.await
			}?;
			std::process::exit(0);
		};

		log!(Level::INFO, "setting slash commands");
		if let Some(guild_id) = config.guild_id {
			http.set_guild_commands(guild_id, commands::commands())?
				.await
		} else {
			http.set_global_commands(commands::commands())?.await
		}
		.context("setting slash commands failed")?;

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

	/// Process an event stream using [`event::process`].
	async fn run(&'static self, mut events: Events) {
		log!(Level::INFO, "started main event stream loop");
		while let Some((_, event)) = events.next().await {
			tokio::spawn(event::process(self, event));
		}
		log!(Level::ERROR, "event stream exhausted");
	}

	fn down(&self) {
		self.cluster.down();
	}
}

trait InMemoryCacheExt {
	/// Tries to retreive a [`VoiceChannel`] from a [`ChannelId`].
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
