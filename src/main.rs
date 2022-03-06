//! Bot that on channel, member & role updates goes through the relevant voice channels
//! in the guild and removes members lacking connection permission.

use std::{env, ffi::OsStr, fs, io::ErrorKind, ops::Deref, path::PathBuf};

use anyhow::Context;
use futures_util::{stream::FuturesUnordered, StreamExt};
use search::Search;
use tokio::signal::unix::{signal, SignalKind};
use tracing::Level;
use tracing_subscriber::EnvFilter;
use twilight_cache_inmemory::{InMemoryCache, ResourceType};
use twilight_gateway::{cluster::Events, Cluster, EventTypeFlags, Intents};
use twilight_http::{client::InteractionClient, Client as HttpClient};
use twilight_model::{
	channel::{ChannelType, GuildChannel, VoiceChannel},
	guild::Permissions as TwilightPermissions,
	id::{
		marker::{ApplicationMarker, ChannelMarker, GuildMarker, UserMarker},
		Id,
	},
};

mod commands;
mod event;
mod interaction;
mod search;

// TODO: try to upstream this to some systemd crate
/// Systemd credential loader helper.
///
/// <https://www.freedesktop.org/software/systemd/man/systemd.exec.html#Credentials>
pub struct CredentialLoader {
	dir: PathBuf,
}

impl CredentialLoader {
	/// Initiate a new loader, returns [`None`] if no credentials are available.
	pub fn new() -> Option<Self> {
		let dir = PathBuf::from(env::var_os("CREDENTIALS_DIRECTORY")?);

		Some(Self { dir })
	}

	/// Get a credential by its ID.
	pub fn get<K: AsRef<OsStr>>(&self, id: K) -> Option<Vec<u8>> {
		self._get(id.as_ref())
	}

	fn _get(&self, id: &OsStr) -> Option<Vec<u8>> {
		let path: PathBuf = [self.dir.as_ref(), id].iter().collect();

		match fs::read(path) {
			Ok(bytes) => Some(bytes),
			Err(e) => {
				if e.kind() == ErrorKind::NotFound {
					None
				} else {
					unreachable!("unexpected io error: {:?}", e)
				}
			}
		}
	}
}

struct Config {
	guild_id: Option<Id<GuildMarker>>,
	remove_commands: bool,
	token: String,
}

/// [`ChannelType`]s the bot operates on.
const MONITORED_CHANNEL_TYPES: ChannelType = ChannelType::GuildVoice;

/// Discord permissions for various actions.
struct Permissions;

impl Permissions {
	/// Required permission to monitor / manage channel.
	const ADMIN: TwilightPermissions = TwilightPermissions::MOVE_MEMBERS;
	/// Required permission to remain connected (avoid being kicked).
	const CONNECT: TwilightPermissions = TwilightPermissions::CONNECT;
}

pub struct Symbol;

impl Symbol {
	/// <https://emojipedia.org/warning/>
	pub const WARNING: &'static str = "\u{26A0}\u{FE0F}";
	pub const BULLET_POINT: &'static str = "\u{2022}";
}

fn app() -> clap::Command<'static> {
	clap::command!().args([
		clap::arg!(--"guild-id" [ID] "Change commands of this guild")
			.env("GUILD_ID")
			.forbid_empty_values(true),
		clap::arg!(--"remove-commands" "Remove commands and exit").env("REMOVE_COMMANDS"),
	])
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
	let matches = app().get_matches();

	let guild_id = match matches.value_of_t::<Id<_>>("guild-id") {
		Ok(g) => Some(g),
		Err(e) if e.kind() == clap::ErrorKind::ArgumentNotFound => None,
		Err(e) => e.exit(),
	};

	let token = if let Some(bytes) = CredentialLoader::new().and_then(|loader| loader.get("token"))
	{
		String::from_utf8(bytes)?.trim_end().to_owned()
	} else {
		eprintln!("systemd credential \"TOKEN\" missing: falling back to environment variable");
		env::var("TOKEN")?
	};

	let config = Config {
		guild_id,
		remove_commands: matches.is_present("remove-commands"),
		token,
	};

	// prefer RUST_LOG with `info` as fallback.
	tracing_subscriber::fmt()
		.with_env_filter(
			EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
		)
		.init();

	let (bot, events) = Bot::new(config).await.context("startup failed")?;

	tokio::spawn(bot.connect());

	let mut sigint = signal(SignalKind::interrupt())?;
	let mut sigterm = signal(SignalKind::terminate())?;

	tokio::select! {
		_ = bot.process(events) => tracing::event!(Level::WARN, "event stream unexpectedly exhausted"),
		_ = sigint.recv() => tracing::event!(Level::INFO, "received SIGINT"),
		_ = sigterm.recv() => tracing::event!(Level::INFO, "received SIGTERM"),
	};

	tracing::event!(Level::INFO, "shutting down");

	bot.shutdown();
	Ok(())
}

pub struct BotRef {
	pub application_id: Id<ApplicationMarker>,
	pub cache: InMemoryCache,
	pub cluster: Cluster,
	pub http: HttpClient,
	/// User ID of the bot
	pub id: Id<UserMarker>,
}

#[derive(Clone, Copy)]
pub struct Bot(&'static BotRef);

impl Bot {
	/// Creates a [`Bot`] and an [`Events`] stream from [`Config`].
	async fn new(config: Config) -> Result<(Self, Events), anyhow::Error> {
		let http = HttpClient::new(config.token.clone());

		let application_id = http
			.current_user_application()
			.exec()
			.await?
			.model()
			.await?
			.id;

		let interaction = http.interaction(application_id);

		// run before starting cluster
		if config.remove_commands {
			if let Some(guild_id) = config.guild_id {
				tracing::event!(Level::INFO, %guild_id, "removing guild slash commands");
				interaction.set_guild_commands(guild_id, &[]).exec()
			} else {
				tracing::event!(Level::INFO, "removing global slash commands");
				interaction.set_global_commands(&[]).exec()
			}
			.await?;

			std::process::exit(0);
		};

		if let Some(guild_id) = config.guild_id {
			tracing::event!(Level::INFO, %guild_id, "setting guild slash commands");
			interaction
				.set_guild_commands(guild_id, &commands::get())
				.exec()
		} else {
			tracing::event!(Level::INFO, "setting global slash commands");
			interaction.set_global_commands(&commands::get()).exec()
		}
		.await?;

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
			let events = EventTypeFlags::GUILDS ^ EventTypeFlags::CHANNEL_PINS_UPDATE
				| EventTypeFlags::GUILD_MEMBERS
				| EventTypeFlags::INTERACTION_CREATE
				| EventTypeFlags::READY
				| EventTypeFlags::GUILD_VOICE_STATES;
			Cluster::builder(config.token, intents)
				.event_types(events)
				.build()
				.await?
		};

		let id = http.current_user().exec().await?.model().await?.id;

		Ok((
			// Arc is a slower alternative since a new task is spawned for
			// incomming events (requiring clone).
			Self(Box::leak(Box::new(BotRef {
				application_id,
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
		tracing::event!(Level::INFO, "all shards connected");
	}

	fn as_interaction(&self) -> InteractionClient {
		self.http.interaction(self.application_id)
	}

	/// Returns `true` if the voice channel is monitored.
	fn is_monitored(self, channel_id: Id<ChannelMarker>) -> bool {
		self.cache
			.permissions()
			.in_channel(self.id, channel_id)
			.expect("resources are available")
			.contains(Permissions::ADMIN)
	}

	/// Listens and processes each [`Event`] from the [`Events`] stream in a new task.
	///
	/// [`Event`]: twilight_model::gateway::event::Event
	async fn process(self, mut events: Events) {
		tracing::event!(Level::INFO, "started event stream loop");
		while let Some((_, event)) = events.next().await {
			tokio::spawn(event::process(self, event));
		}
	}

	/// Removes users, logging on error.
	///
	/// Returns the number of users removed.
	async fn remove(
		self,
		guild_id: Id<GuildMarker>,
		users: impl Iterator<Item = Id<UserMarker>>,
	) -> usize {
		async fn remove(bot: Bot, guild_id: Id<GuildMarker>, user_id: Id<UserMarker>) {
			tracing::event!(Level::INFO, user.id = %user_id, "kicking");
			if let Err(e) = bot
				.http
				.update_guild_member(guild_id, user_id)
				.channel_id(None)
				.exec()
				.await
			{
				tracing::event!(Level::ERROR, error = &e as &dyn std::error::Error);
			}
		}

		let mut futures = users
			.map(|user_id| remove(self, guild_id, user_id))
			.collect::<FuturesUnordered<_>>();
		let mut processed = 0;
		while futures.next().await.is_some() {
			processed += 1;
		}
		processed
	}

	/// Conveniant constructor of [`Search`].
	const fn search(self, guild_id: Id<GuildMarker>) -> Search {
		Search {
			bot: self,
			guild_id,
		}
	}

	fn shutdown(self) {
		self.cluster.down();
	}
}

impl Deref for Bot {
	type Target = BotRef;

	fn deref(&self) -> &Self::Target {
		self.0
	}
}

trait InMemoryCacheExt {
	/// Returns a [`GuildChannel::Voice`] from a [`Id<ChannelMarker>`].
	fn voice_channel(&self, channel_id: Id<ChannelMarker>) -> Option<VoiceChannel>;
}

impl InMemoryCacheExt for InMemoryCache {
	fn voice_channel(&self, channel_id: Id<ChannelMarker>) -> Option<VoiceChannel> {
		match self.guild_channel(channel_id)?.value().resource() {
			GuildChannel::Voice(c) => Some(c.clone()),
			_ => None,
		}
	}
}

#[cfg(test)]
mod tests {
	#[test]
	fn verify_app() {
		super::app().debug_assert()
	}
}
