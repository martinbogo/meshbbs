# Games

Meshbbs includes optional, lightweight games you can access from the public channel. They’re designed to be low‑traffic and fun without overwhelming the mesh.

## 🎰 Slot Machine (public channel)

- Commands:
  - `^SLOT` / `^SLOTMACHINE` — spin once; the BBS broadcasts the result on the public channel (best‑effort)
  - `^SLOTSTATS` — show your coins, spins, wins, and jackpots
- Economy:
  - Each spin costs 5 coins
  - New players start with 100 coins
  - If your balance reaches 0, you’ll be refilled to 100 after ~24 hours
- Payouts (multiplier × bet):
  - 7️⃣7️⃣7️⃣ = JACKPOT (progressive pot, minimum 500 coins; grows by 5 coins per losing spin), 🟦🟦🟦 ×50, 🔔🔔🔔 ×20, 🍇🍇🍇 ×14, 🍊🍊🍊 ×10, 🍋🍋🍋 ×8, 🍒🍒🍒 ×5
  - Two 🍒 ×3, one 🍒 ×2, otherwise ×0
- Visibility and reliability:
  - Results are broadcast to the public channel for room visibility (best‑effort)
  - Broadcasts may request an ACK and are considered successful when at least one ACK is received within a short window (no retries)
- Persistence: Player balances and stats are stored under `data/slotmachine/players.json`

Tip: If you see “Out of coins… Next refill in ~Hh Mm”, check back later or run `^SLOTSTATS` to see your current balance and stats.

---

## 🎱 Magic 8‑Ball (public channel)

- Command:
  - `^8BALL` — ask a yes/no question and receive a classic Magic 8‑Ball response
- Behavior:
  - Stateless and lightweight; no persistence
  - Broadcast-only on the public channel (best‑effort)
- Reliability:
  - Broadcasts may request an ACK and are considered successful when at least one ACK is received within a short window (no retries)

---

## 🔮 Fortune Cookies (public channel)

- Command:
  - `^FORTUNE` — receive a random fortune from classic Unix wisdom databases
- Behavior:
  - Stateless; draws from 140 curated fortunes including programming quotes, philosophy, literature, and humor
  - All fortunes under 200 characters for mesh-friendly transmission
  - Broadcast-only on the public channel (best‑effort)
  - 5-second cooldown per node to prevent spam
- Content:
  - Classic Unix fortune database entries
  - Programming and technology wisdom
  - Motivational quotes and life philosophy
  - Clean humor and wit
- Quality Assurance:
  - Comprehensive unit test coverage (11+ tests)
  - Thread safety validation
  - Content quality checks and character validation
  - Randomness and distribution testing
- Reliability:
  - Same broadcast behavior as Magic 8‑Ball

> 💡 **Developer Note**: The Fortune module includes extensive documentation and testing. See [`docs/development/fortune-module.md`](../development/fortune-module.md) for implementation details.

---

More games may be added over time. Have an idea? Open a GitHub issue or discussion!