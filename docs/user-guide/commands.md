# Command Reference

Complete reference for all meshbbs commands available to users.

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
| `M` | Quick navigation to message topics | `M` |
| `WHERE` or `W` | Show current breadcrumb path | `WHERE` |
| `U` | Quick navigation to user menu | `U` |
| `Q` | Quit/logout | `Q` |
| `B` | Back to previous menu | `B` |

### Message Topics

| Command | Description | Example |
|---------|-------------|---------|
| `TOPICS` or `LIST` | List available message topics | `TOPICS` |
| `READ topic` | Read recent messages from topic | `READ general` |
| `POST topic message` | Post a message to topic | `POST general Hello everyone!` |
| `POST topic` | Start multi-line post | `POST general` |

#### Topics and Subtopics (Compact UI)

- Press `M` to open Topics (root topics only are shown)
- Items with children show a `›` marker; selecting opens Subtopics
- In Subtopics:
	- `1-9` pick subtopic on current page; nested levels are supported
	- `U` or `B` goes up one level; `M` returns to root Topics; `L` shows more
	- Selecting a leaf subtopic enters Threads

#### Multi-line Posting

When using `POST topic` without a message, you enter multi-line mode:

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
| `DELETE topic id` | Remove a message | `DELETE general msg123` |
| `LOCK topic` | Prevent new posts in topic | `LOCK general` |
| `UNLOCK topic` | Allow posts in topic again | `UNLOCK general` |
| `DELLOG [page]` / `DL [page]` | View deletion audit log | `DELLOG`, `DL`, or `DL 2` |

## Sysop Commands (Level 10)

Available only to system operators:

| Command | Description | Example |
|---------|-------------|---------|
| `PROMOTE user` | Increase user's access level | `PROMOTE alice` |
| `DEMOTE user` | Decrease user's access level | `DEMOTE bob` |

## Dynamic Prompts

Meshbbs shows contextual prompts that reflect your current state:

| Prompt | Meaning |
|--------|---------|
| `unauth>` | Not logged in |
| `alice (lvl1)>` | Logged in as alice, user level 1 |
| `alice@general>` | Reading messages in 'general' topic |
| `post@general>` | Posting a message to 'general' topic |
| `alice@community>` → `alice (lvl1)>` | Using `B`/`U` goes up from Threads to Subtopics, then to Topics |

## Tips and Shortcuts

- **First-time help**: The first time you use `HELP` after login, you'll see a shortcuts reminder
- **Topic names**: Long topic names are truncated in prompts with ellipsis
- **Message limits**: Each message is limited to 230 bytes (optimized for Meshtastic)
	- The server clamps outputs UTF‑8 safely and appends the prompt; the total frame (body + prompt) ≤ 230 bytes.
    - Long thread reads are automatically split across multiple messages; the interactive prompt appears only on the final part.
- **Session timeout**: Sessions automatically timeout after inactivity (configurable by sysop)
- **Case sensitivity**: Commands are case-insensitive (`help`, `HELP`, and `Help` all work)

## Error Messages

Common error messages and their meanings:

| Error | Meaning | Solution |
|-------|---------|----------|
| `Invalid username` | Username doesn't meet requirements | Use 2-20 chars, letters/numbers/underscore only |
| `Wrong password` | Incorrect password provided | Check password or use `SETPASS` if passwordless |
| `Topic not found` | Message topic doesn't exist | Use `TOPICS` to see available topics |
| `Access denied` | Insufficient privileges | Check your user level with sysop |
| `Message too long` | Message exceeds 230 byte limit | Shorten your message |
| `Session timeout` | Inactive too long | Log in again |

## Examples
### Compact Single-Letter Flow (DM Session)

```
> M
[Meshbbs] Topics
1. hello  2. general  3. technical
Type number to select topic. L more. H help. X exit
alice (lvl1)>

> 1
Messages in hello:
[BBS][hello] Threads
1 Test Title  2 Intro…
Reply: 1-9 read, N new, L more, B back, F <text> filter
alice@hello>

> N
[BBS] New thread title (≤32):
post@hello>

> Test Title
Body: (single message)
post@hello>

> This is the body of the test thread.
Messages in hello:
[BBS][hello] Threads
1 Test Title
Reply: 1-9 read, N new, L more, B back, F <text> filter
alice@hello>

> 1
[BBS][hello > Test Title] p1/1
This is the body of the test thread.
Reply: + next, - prev, Y reply, B back, H help
alice@hello>

> W
[BBS] You are at: Meshbbs > Topics > hello > Read
alice@hello>
```

### Basic Session Flow

```
Public channel:
> ^LOGIN alice
< Meshbbs: Pending login for 'alice'. Open a DM to start your session.

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
mod (lvl5)> DL 2
< Recent deletions:
< 2025-09-23 10:30 - general/msg456 deleted by mod
< 2025-09-23 09:15 - announcements/msg789 deleted by admin
mod (lvl5)> LOCK general
< Area 'general' is now locked to new posts
mod (lvl5)> UNLOCK general  
< Area 'general' is now open for posts
```