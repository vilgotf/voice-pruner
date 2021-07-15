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
A container image is availiable at [`ghcr.io/vilgotf/voice-pruner`][container] (built using GitHub [actions](.github/workflows/publish.yml)).

[bot_invite_link]: https://discord.com/api/oauth2/authorize?client_id=861223160905072640&permissions=16777216&scope=bot%20applications.commands
[container]: https://github.com/users/vilgotf/packages/container/package/voice-pruner