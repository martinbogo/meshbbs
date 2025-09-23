# Command Reference

Complete reference for all MeshBBS commands available to users.

## Connection Commands

### Initial Discovery (Public Channel)

These commands are used on the public Meshtastic channel and require the `^` prefix:

| Command | Description | Example |
|---------|-------------|---------|
| `^HELP` | Get basic information about the BBS | `^HELP` |
| `^LOGIN username` | Register for a private session | `^LOGIN alice` |

## Session Commands (Direct Message)

After using `^LOGIN` on the public channel, open a direct message to the BBS node to access these commands:

### Authentication

| Command | Description | Example |
|---------|-------------|---------|
| `LOGIN username [password]` | Log in (sets password if first time) | `LOGIN alice mypass` |
| `REGISTER username password` | Create new account | `REGISTER bob secret123` |
| `LOGOUT` | End current session | `LOGOUT` |
| `CHPASS old new` | Change password | `CHPASS oldpass newpass` |
| `SETPASS new` | Set password (for passwordless accounts) | `SETPASS mypassword` |

### Help and Navigation

| Command | Description | Example |
|---------|-------------|---------|
| `HELP` or `H` or `?` | Show compact help | `HELP` |
| `HELP+` or `HELP V` | Show detailed help with examples | `HELP+` |
| `M` | Quick navigation to message areas | `M` |
| `U` | Quick navigation to user menu | `U` |
| `Q` | Quit/logout | `Q` |
| `B` | Back to previous menu | `B` |

### Message Areas

| Command | Description | Example |
|---------|-------------|---------|
| `AREAS` or `LIST` | List available message areas | `AREAS` |
| `READ area` | Read recent messages from area | `READ general` |
| `POST area message` | Post a message to area | `POST general Hello everyone!` |
| `POST area` | Start multi-line post | `POST general` |

#### Multi-line Posting

When using `POST area` without a message, you enter multi-line mode:

```
> POST general
Enter your message. End with '.' on a new line:
This is a multi-line message.
You can write several lines.
End with a period on its own line.
.
Message posted successfully!
```

### User Commands

| Command | Description | Example |
|---------|-------------|---------|
| `CHPASS old new` | Change your password | `CHPASS oldpass newpass` |
| `SETPASS new` | Set initial password | `SETPASS mypassword` |

## Moderator Commands (Level 5+)

Available to users with moderator privileges:

| Command | Description | Example |
|---------|-------------|---------|
| `DELETE area id` | Remove a message | `DELETE general msg123` |
| `LOCK area` | Prevent new posts in area | `LOCK general` |
| `UNLOCK area` | Allow posts in area again | `UNLOCK general` |
| `DELLOG [page]` | View deletion audit log | `DELLOG` or `DELLOG 2` |

## Sysop Commands (Level 10)

Available only to system operators:

| Command | Description | Example |
|---------|-------------|---------|
| `PROMOTE user` | Increase user's access level | `PROMOTE alice` |
| `DEMOTE user` | Decrease user's access level | `DEMOTE bob` |

## Dynamic Prompts

MeshBBS shows contextual prompts that reflect your current state:

| Prompt | Meaning |
|--------|---------|
| `unauth>` | Not logged in |
| `alice (lvl1)>` | Logged in as alice, user level 1 |
| `alice@general>` | Reading messages in 'general' area |
| `post@general>` | Posting a message to 'general' area |

## Tips and Shortcuts

- **First-time help**: The first time you use `HELP` after login, you'll see a shortcuts reminder
- **Area names**: Long area names are truncated in prompts with ellipsis
- **Message limits**: Each message is limited to 230 bytes (optimized for Meshtastic)
- **Session timeout**: Sessions automatically timeout after inactivity (configurable by sysop)
- **Case sensitivity**: Commands are case-insensitive (`help`, `HELP`, and `Help` all work)

## Error Messages

Common error messages and their meanings:

| Error | Meaning | Solution |
|-------|---------|----------|
| `Invalid username` | Username doesn't meet requirements | Use 2-20 chars, letters/numbers/underscore only |
| `Wrong password` | Incorrect password provided | Check password or use `SETPASS` if passwordless |
| `Area not found` | Message area doesn't exist | Use `AREAS` to see available areas |
| `Access denied` | Insufficient privileges | Check your user level with sysop |
| `Message too long` | Message exceeds 230 byte limit | Shorten your message |
| `Session timeout` | Inactive too long | Log in again |

## Examples

### Basic Session Flow

```
Public channel:
> ^LOGIN alice
< MeshBBS: Pending login for 'alice'. Open a DM to start your session.

Direct message:
> LOGIN alice mypassword
< Welcome alice! Type HELP for commands.
alice (lvl1)> AREAS
< Available areas: general, announcements
alice (lvl1)> READ general
< [Recent messages from general area...]
alice@general> POST general Hello everyone from the mesh!
< Message posted successfully!
alice@general> Q
< Goodbye!
```

### Moderator Example

```
mod (lvl5)> DELLOG
< Recent deletions:
< 2025-09-23 10:30 - general/msg456 deleted by mod
< 2025-09-23 09:15 - announcements/msg789 deleted by admin
mod (lvl5)> LOCK general
< Area 'general' is now locked to new posts
mod (lvl5)> UNLOCK general  
< Area 'general' is now open for posts
```