---
title: Real‑World QA Test Plan
description: Printer‑friendly checklist to validate meshbbs on a live Meshtastic device and shared mesh.
---

# Real‑World QA Test Plan

Use this printer‑friendly checklist to validate meshbbs behavior against a live Meshtastic device and the shared mesh. Keep a notes doc during testing with exact commands sent, observed responses, and relevant `meshbbs.log` snippets.

Tip for printing: Use your browser’s Print function to save this page as a PDF.

## Setup

### Prereqs

- [ ] One Meshtastic device connected and configured to the same channel as your BBS
- [ ] macOS Terminal (zsh)
- [ ] Current repo builds and tests pass

### Build and run

- [ ] Build: `cargo build --release`
- [ ] Start: `./target/release/meshbbs start`
- [ ] Confirm server logs show your Meshtastic port and that the device is connected

### Data safety

- [ ] Backup confirmed: `config.toml` and `data/topics.json`
- [ ] `config.toml` contains a valid sysop name and password set (see `meshbbs sysop-passwd`)

Note: For any problem found, capture the exact command, what happened, and relevant excerpts from `meshbbs.log`.

## Public channel (shared mesh) sanity

### `^HELP` (from your Meshtastic client)

Expect:
- [ ] A DM arrives with compact help guidance
- [ ] A delayed public notice may appear after the DM (scheduler delay)

Check:
- [ ] DM contains a concise help sentence (not verbose), fits in one frame with prompt
- [ ] You see the prompt at the end of the DM: `unauth>` (if not logged in)

### `^LOGIN alice`

Expect:
- [ ] Public reply indicates login pending for `alice`
- [ ] You’re told to open DM to continue

### `^WEATHER` (optional)

Expect:
- [ ] Either a broadcast weather message (if allowed/available), or a DM fallback

Check:
- [ ] No floods; messages obey pacing. No rate‑limit errors in logs

## Direct Message: Authentication

### `REGISTER alice secretPass1`

Expect:
- [ ] Success message; a welcome/unread summary that fits in one frame
- [ ] Prompt changes to `alice (lvl1)>`

Check:
- [ ] First HELP after login shows a single shortcuts line once: `Shortcuts: M=areas U=user Q=quit`

### `HELP` (compact)

Expect:
- [ ] Single‑frame compact help including:
  - ACCT: `SETPASS`, `CHPASS`, `LOGOUT`
  - MSG navigation hints (`M` topics; digits pick; `+`/`-`; `F`; `READ`/`POST`/`TOPICS`)
  - OTHER: `WHERE | U | Q`
- [ ] Fits in 230 bytes including prompt

### `HELP+` (verbose)

Expect:
- [ ] Multiple DM chunks; prompt appended to the last chunk only
- [ ] Detailed sections for navigation, moderator/sysop, legacy commands

### `SETPASS newPassOnlyIfPasswordless` (only if the account had no password)

Expect:
- [ ] Success (or an explanatory message if already set)

### `CHPASS secretPass1 secretPass2`

Expect:
- [ ] Success; future logins require the new password on unbound nodes

### `LOGOUT` then `LOGIN alice secretPass2`

Expect:
- [ ] Login success; prompt is `alice (lvl1)>`
- [ ] Wrong password attempt yields an error

### `WHERE` (or `W`)

Expect:
- [ ] Breadcrumb reporting where you are (Login/Main/Topics/etc)

## Compact UI: Topics → Subtopics → Threads → Read

### `M` (go to Topics)

Expect:
- [ ] List of root topics only
- [ ] Items with children show a `›` marker
- [ ] Footer provides short instructions (L more, H help, X exit)

### Select a topic with subtopics (press a digit)

Expect:
- [ ] Subtopics view listing child topics (with `›` if nested)
- [ ] `U`/`B` goes up one level; `M` returns to root Topics; `L` paginates

### Select a leaf subtopic (press a digit)

Expect:
- [ ] Threads list for that subtopic
- [ ] Pinned threads show a `📌` icon
- [ ] If the topic is locked, header shows `[locked]`
- [ ] Footer: `Reply: 1-9 read, N new, L more, B back, F <text> filter` (if no filter applied)

### `N` (create a new thread)

Expect:
- [ ] Prompt for title (≤32)
- [ ] Then prompt for body (single‑message)
- [ ] Return to Threads with your new thread listed (title shown, truncated if needed)

### `F <text>` (filter) and `F` (clear)

Expect:
- [ ] Filter narrows visible thread titles; `F` with no text clears
- [ ] `L` paginates filtered results if many

### `1` (read first thread)

Expect:
- [ ] Read view shows `[BBS][topic > title] …` and body (truncated for space)
- [ ] If a reply exists, last reply line preview is shown prefixed with `—`
- [ ] Footer: `+` next, `-` prev, `Y` reply, `B` back, `H` help

### `Y` (reply), then type a short reply

Expect:
- [ ] Return to Read view; your reply is saved and previewed on subsequent displays

### `+` and `-` (navigate between threads)

Expect:
- [ ] Moves to next/previous thread; prompt and headers update accordingly

### `B` (from Read)

Expect:
- [ ] Returns to Threads
- [ ] Another `B` goes up (Threads → Subtopics; Subtopics → Topics); `M` always returns to Topics

### `X` (exit)

Expect:
- [ ] Ends the session (`Goodbye! 73s`) and prompt no longer updates

## Legacy inline commands (still supported)

### `TOPICS` / `LIST`

Expect:
- [ ] A listing of topics and descriptions (respects read permissions)

### `READ general` (or another topic)

Expect:
- [ ] Lists recent messages from the topic (top N)
- [ ] Format: `author | timestamp, content …` with separators

### `POST general Hello world!`

Expect:
- [ ] Posted to that topic immediately (unless locked or permission denied)

### `POST general` (multi‑line)

Expect:
- [ ] Instructions for multi‑line; end with a dot on its own line to post

### Unknown command (e.g., `NOPE`)

Expect:
- [ ] Terse reply: `Invalid command "NOPE"` followed by the prompt
- [ ] Stays within frame limit

## Moderator features (level ≥5)

### Login as sysop and promote `alice` to moderator

As sysop (login as your configured sysop account), then:

- `PROMOTE alice`
- or `G @alice=5` (equivalent)

Expect:
- [ ] Confirmation; role changes reflected in subsequent HELP output for `alice`

### Moderator in Threads: Pin/Unpin, Rename, Lock/Unlock

In Threads list:

- `P1` (toggle pinned for item 1)
- `R1 New Title` (rename thread title; truncates to 32)
- `K` (toggle lock state for current topic; also try `LOCK` and `UNLOCK` explicitly)

Expect:
- [ ] Visual changes in list headers: `📌` appears; `[locked]` on header when locked
- [ ] Posting while locked is denied; unlocking restores posting

### Delete with confirmation

- `D1` (delete item 1) → Confirm delete? (Y/N)

Expect:
- [ ] `Y` deletes and returns to Threads; `N` cancels
- [ ] Deletion removes the message and logs to audit

### Deletion audit log

- `DELLOG` (and optionally `DELLOG 2` for page 2)

Expect:
- [ ] Entries including timestamp, topic/id, and actor

## Admin/sysop dashboards and user admin (level ≥10)

### `USERS [pattern]`

Expect:
- [ ] List of users; optional filtering when pattern is provided

### `USERINFO alice`

Expect:
- [ ] Detailed info: level, posts, registered date

### `WHO` / `SESSIONS`

Expect:
- [ ] Basic summaries (if supported in your build; some are placeholders)

### `BROADCAST <message>`

Expect:
- [ ] Broadcast to public; DM fallback possible or admin‑log entry

### `SYSLOG INFO Hello mesh!`

Expect:
- [ ] Logged to security/audit log with the indicated level; success reply

### `ADMIN` or `DASHBOARD`

Expect:
- [ ] Quick metrics snapshot: total users, total messages, moderators, recent registrations, etc.

## Permissions/denials

### As a regular user (level 1)

Try moderator commands (`DELETE`, `LOCK/UNLOCK`, `DELLOG`) and sysop commands (`PROMOTE/DEMOTE`, `SYSLOG`, `BROADCAST`).

Expect:
- [ ] “Permission denied.” for restricted commands; HELP hides these unless authorized

## Size and prompt checks (sanity)

### DM length checks (visual)

Observe that:

- [ ] `HELP` fits in one frame with prompt
- [ ] Registration/login banners are concise and never get cut mid‑character
- [ ] `HELP+`: prompt only on last chunk

## Persistence checks

### Restart BBS

Stop and start the server.

Expect:
- [ ] Topics, subtopics, locked states, and messages persist (`data/topics.json` and files under `data/messages/…`)
- [ ] Users persist; logging back in works with saved password (if set)

## Optional: Weather caching sanity

### Send `^WEATHER` multiple times

Expect:
- [ ] Caching in effect; public broadcast or DM fallback; no spam; pacing respected

## Optional: Negative/edge cases

### Topic permission checks

Create a topic that requires higher read/post levels, then attempt actions as a lower‑level user.

Expect:
- [ ] Topic hidden when below `read_level`; post denied when below `post_level`

### Over‑long post content

Try to post a very long message via legacy `POST` (simulate >10KB) or with invalid characters.

Expect:
- [ ] Error about invalid characters or size limit; no message stored

### Invalid usernames on `REGISTER`/`LOGIN`

Expect:
- [ ] Clear validation error with guidance; no user created

## Operational tips

- [ ] Use `WHERE`/`W` often for breadcrumb clarity at each level you test
- [ ] For subtopics, verify multi‑level nesting by creating a child of a child, then navigating up with `B`/`U`/`M`
- [ ] For moderator actions, verify that UI hints appear in HELP only when the role is high enough

---

Source on GitHub: This plan reflects the current implementation documented in the project README and command reference.
