# Voice pruner

A very simple discord bot which monitors voice channels and on channel, member and role update removes members lacking `CONNECT` permission.

I host a free to use instance of this bot [here][bot_invite_link].

## Commands
* List
  * Monitored - lists monitored channels
  * Unmonitored - lists unmonitored channels

## Permissions
The bot monitors all channels it has the `MOVE_MEMBERS` permission in.
Use channel overwrites for a denylist or remove the `MOVE_MEMBERS` global permission for an allowlist.

### Privileged intents
Requires the `GUILD_MEMBERS` intent for monitoring the removal of members roles.

## Use case
This bot is primarily meant as a template or showcase of twilight.
That said, it could be combined with some permission manager to immediately remove members from voice channels.

## Self hosting
Compiled binaries can be found in the actions page, under artifacts.

### Systemd (prefered)
The bot tries to use systemd's credential storage (v247+) to read the bot token and falls back to using the `TOKEN` env variable.
For an example of how to use credintial storage, look at the provided [systemd service file](voice-pruner.service).

### Container
A container image is also availiable at [`ghcr.io/vilgotf/voice-pruner`][container].

[bot_invite_link]: https://discord.com/api/oauth2/authorize?client_id=861223160905072640&permissions=16777216&scope=bot%20applications.commands
[container]: https://github.com/users/vilgotf/packages/container/package/voice-pruner