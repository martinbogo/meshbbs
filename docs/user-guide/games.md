# Games

Meshbbs includes optional, lightweight games you can access from the public channel. They’re designed to be low‑traffic and fun without overwhelming the mesh.

## 🎰 Slot Machine (public channel)

- Commands:
  - `^SLOT` / `^SLOTMACHINE` — spin once and broadcast the result (with DM fallback)
  - `^SLOTSTATS` — show your coins, spins, wins, and jackpots
- Economy:
  - Each spin costs 5 coins
  - New players start with 100 coins
  - If your balance reaches 0, you’ll be refilled to 100 after ~24 hours
- Payouts (multiplier × bet):
  - 7️⃣7️⃣7️⃣ ×100 (jackpot), 🟦🟦🟦 ×50, 🔔🔔🔔 ×20, 🍇🍇🍇 ×14, 🍊🍊🍊 ×10, 🍋🍋🍋 ×8, 🍒🍒🍒 ×5
  - Two 🍒 ×3, one 🍒 ×2, otherwise ×0
- Visibility: Results are broadcast to the mesh; if broadcast can’t be sent immediately, a DM fallback is used
- Persistence: Player balances and stats are stored under `data/slotmachine/players.json`

Tip: If you see “Out of coins… Next refill in ~Hh Mm”, check back later or run `^SLOTSTATS` to see your current balance and stats.

---

More games may be added over time. Have an idea? Open a GitHub issue or discussion!