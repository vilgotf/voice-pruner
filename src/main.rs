//! Bot that on channel, member & role updates goes through the relevant voice channels
//! in the guild and removes members lacking connection permission.

use std::{
	env,
	error::Error,
	ffi::{OsStr, OsString},
	fmt,
	fs::File,
	io::{ErrorKind, Read},
	ops::Deref,
	os::unix::ffi::OsStringExt,
	path::PathBuf,
};

use anyhow::{anyhow, Context};
use clap::{crate_authors, crate_description, crate_license, crate_name, crate_version, App, Arg};
use futures_util::{stream::FuturesUnordered, StreamExt};
use search::Search;
use tokio::signal::unix::{signal, SignalKind};
use tracing::{event as log, Level};
use tracing_subscriber::EnvFilter;
use twilight_cache_inmemory::{InMemoryCache, ResourceType};
use twilight_gateway::{cluster::Events, Cluster, EventTypeFlags, Intents};
use twilight_http::{error::Error as HttpError, Client as HttpClient};
use twilight_model::{
	channel::{GuildChannel, VoiceChannel},
	guild::Permissions,
	id::{ChannelId, GuildId, UserId},
	voice::VoiceState,
};

mod command;
mod event;
mod interaction;
mod response;
mod search;

#[derive(Clone, Copy)]
pub struct Bot(&'static BotRef);

impl Deref for Bot {
	type Target = BotRef;

	fn deref(&self) -> &Self::Target {
		self.0
	}
}

pub struct BotRef {
	pub cache: InMemoryCache,
	pub cluster: Cluster,
	pub http: HttpClient,
	/// User ID of the bot
	pub id: UserId,
}

// TODO: try to upstream this to some systemd crate
/// Retrieves the credential `key` from systemd's service credential manager.
///
/// # Errors
/// Errors if the credential is not present.
/// Errors if systemd's credential manager is not enabled.
///
/// # Panics
/// This function panics on IO errors.
///
/// # Examples
///
/// ```
/// use credential;
///
/// let key = "token";
/// match credential(key) {
///		Ok(val) => println!("{}: {:?}", key, val),
///		Err(e) => println!("couldn't fetch {}: {}", key, e),
/// }
/// ```
fn credential<K: AsRef<OsStr>>(key: K) -> Result<OsString, CredentialError> {
	_credential(key.as_ref())
}

fn _credential(key: &OsStr) -> Result<OsString, CredentialError> {
	let dir = env::var_os("CREDENTIALS_DIRECTORY").ok_or(CredentialError::Inactive)?;
	let path: PathBuf = [&dir, key].iter().collect();
	match File::open(path) {
		Ok(mut file) => {
			let mut bytes =
				// logic from std::fs::read
				Vec::with_capacity(file.metadata().map(|m| m.len() as usize + 1).unwrap_or(0));
			file.read_to_end(&mut bytes).unwrap();

			Ok(OsStringExt::from_vec(bytes))
		}
		Err(e) => {
			if e.kind() == ErrorKind::NotFound {
				Err(CredentialError::NotPresent)
			} else {
				panic!("io error: {:?}", e)
			}
		}
	}
}

/// The error type for retreiving systemd credentials.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CredentialError {
	/// Credential system was inactive.
	Inactive,
	/// The specified credential was not present.
	NotPresent,
}

impl fmt::Display for CredentialError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			CredentialError::Inactive => f.write_str("systemd credential system inactive"),
			CredentialError::NotPresent => f.write_str("credential not found"),
		}
	}
}

impl Error for CredentialError {}

struct Config {
	guild_id: Option<GuildId>,
	remove_commands: bool,
	token: String,
}

/// Acquires [`Config`] from cmdline using [`clap::App`]
fn conf() -> Result<Config, anyhow::Error> {
	let matches = App::new(crate_name!())
		.about(crate_description!())
		.author(crate_authors!())
		.license(crate_license!())
		.version(crate_version!())
		.args([
			Arg::from("--guild-id [ID] 'Change commands of this guild'")
				.env("GUILD_ID")
				.forbid_empty_values(true),
			Arg::from("--remove-commands 'Remove commands and exit'").env("REMOVE_COMMANDS"),
		])
		.get_matches();

	let guild_id = match matches.value_of_t::<u64>("guild-id") {
		Ok(g) => Some(g.into()),
		Err(e) if e.kind == clap::ErrorKind::ArgumentNotFound => None,
		Err(e) => e.exit(),
	};
	let token = match credential("token") {
		Ok(val) => val
			.into_string()
			.map_err(|s| anyhow!("token contains non unicode data: {:?}", s))?
			.trim_end()
			.to_owned(),
		Err(reason) => {
			log!(Level::WARN, %reason, "using `TOKEN` env variable, prefer loading it with systemd");
			env::var("TOKEN")?
		}
	};
	let remove_commands = matches.is_present("remove-commands");

	Ok(Config {
		guild_id,
		remove_commands,
		token,
	})
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
	// prefer RUST_LOG with `info` as fallback.
	tracing_subscriber::fmt()
		.with_env_filter(
			EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
		)
		.init();

	let (bot, events) = Bot::new(conf()?).await.context("startup failed")?;

	tokio::spawn(bot.connect());

	// Listen to sigint (ctrl-c) and sigterm (docker/podman).
	let mut sigint = signal(SignalKind::interrupt())?;
	let mut sigterm = signal(SignalKind::terminate())?;

	tokio::select! {
		_ = bot.process(events) => log!(Level::WARN, "event stream unexpectedly exhausted"),
		_ = sigint.recv() => log!(Level::INFO, "received SIGINT"),
		_ = sigterm.recv() => log!(Level::INFO, "received SIGTERM"),
	};

	log!(Level::INFO, "shutting down");

	bot.shutdown();
	Ok(())
}

impl Bot {
	/// Creates a [`Bot`] and an [`Events`] stream from [`Config`].
	async fn new(config: Config) -> Result<(Self, Events), anyhow::Error> {
		let http = HttpClient::new(config.token.clone());

		let id = http.current_user().exec().await?.model().await?.id;
		http.set_application_id(id.0.into());

		// run before starting cluster
		if config.remove_commands {
			if let Some(guild_id) = config.guild_id {
				log!(Level::INFO, %guild_id, "removing guild slash commands");
				http.set_guild_commands(guild_id, &[])?.exec().await
			} else {
				log!(Level::INFO, "removing global slash commands");
				http.set_global_commands(&[])?.exec().await
			}?;

			std::process::exit(0);
		};

		if let Some(guild_id) = config.guild_id {
			log!(Level::INFO, %guild_id, "setting guild slash commands");
			http.set_guild_commands(guild_id, &command::commands())?
				.exec()
				.await
		} else {
			log!(Level::INFO, "setting global slash commands");
			http.set_global_commands(&command::commands())?.exec().await
		}?;

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

		Ok((
			// Arc is a slower alternative since a new task is spawned for
			// incomming events (requiring clone).
			Self(Box::leak(Box::new(BotRef {
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
	// default to true since that means they're not kicked
	fn permitted(self, state: &VoiceState) -> bool {
		let channel_id = if let Some(channel_id) = state.channel_id {
			channel_id
		} else {
			log!(Level::WARN, "got state of disconnected user");
			return true;
		};

		self.cache
			.permissions()
			.in_channel(state.user_id, channel_id)
			.log()
			.map(|p| p.contains(Permissions::CONNECT))
			.unwrap_or(true)
	}

	/// Spawns a new task for each [`Event`] in the [`Events`] stream and calls [`event::process`] on it.
	///
	/// [`Event`]: twilight_model::gateway::event::Event
	async fn process(self, mut events: Events) {
		log!(Level::INFO, "started event stream loop");
		while let Some((_, event)) = events.next().await {
			tokio::spawn(event::process(self, event));
		}
	}

	/// Removes users, logging on error.
	async fn remove(self, guild_id: GuildId, users: impl Iterator<Item = UserId>) {
		async fn remove(bot: Bot, guild_id: GuildId, user_id: UserId) -> Result<(), HttpError> {
			log!(Level::INFO, user.id = %user_id, "kicking");
			bot.http
				.update_guild_member(guild_id, user_id)
				.channel_id(None)
				.exec()
				.await?;
			Ok(())
		}

		let mut futures = users
			.map(|user_id| async move { remove(self, guild_id, user_id).await.log() })
			.collect::<FuturesUnordered<_>>();
		while futures.next().await.is_some() {}
	}

	const fn search(self, guild_id: GuildId) -> Search {
		Search {
			bot: self,
			guild_id,
		}
	}

	fn shutdown(self) {
		self.cluster.down();
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

impl<T, E: 'static> Log for Result<T, E>
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
