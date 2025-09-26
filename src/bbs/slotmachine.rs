//! Slot machine mini‑game used by public channel commands ^SLOT and ^SLOTMACHINE.
//!
//! Overview
//! - Emoji reels with fixed distributions and deterministic payout table
//! - Economy: 100 coins starting balance, 5 coins per spin, 24h refill when balance reaches 0
//! - Persistence: JSON file at `<data_dir>/slotmachine/players.json` keyed by Meshtastic node ID
//! - Concurrency: file access guarded with fs2 file locks (shared for read, exclusive for write)
//! - Stats: total spins, wins, jackpots, last spin and last jackpot timestamps
//!
//! Public commands (handled by `bbs::server`):
//! - `^SLOT` / `^SLOTMACHINE` — spin once and broadcast the result (with DM fallback)
//! - `^SLOTSTATS` — show per‑player stats and current coin balance
//!
//! Payouts:
//! - 7️⃣7️⃣7️⃣ = JACKPOT — pays the progressive pot (minimum 500 coins; grows by the bet amount (5 coins) for every losing spin across all players)
//! - 🟦🟦🟦 = ×50
//! - 🔔🔔🔔 = ×20
//! - 🍇🍇🍇 = ×14
//! - 🍊🍊🍊 = ×10
//! - 🍋🍋🍋 = ×8
//! - 🍒🍒🍒 = ×5
//! - Two 🍒 = ×3
//! - One 🍒 = ×2
//! - Otherwise = ×0
//!
//! Jackpot pot: The global jackpot starts at 500 coins and increases by the bet amount for
//! every losing spin across all players. When a player hits 7️⃣7️⃣7️⃣, they win the current
//! jackpot (>= 500). After payout, the jackpot resets back to 500 (loss counter to 0).
//!
//! The module is intentionally self‑contained and exposes a small API that the BBS server calls
//! to perform spins and query player status.

use chrono::{DateTime, Utc, Duration as ChronoDuration};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use fs2::FileExt;
use std::path::{Path, PathBuf};

/// Fixed bet cost per spin (coins deducted before spin)
pub const BET_COINS: u32 = 5;
/// New player grant and daily refill amount (awarded when creating a record or after 24h at 0)
pub const DAILY_GRANT: u32 = 100;
/// Refill cooldown in hours when balance reaches zero
pub const REFILL_HOURS: i64 = 24;

// Reels: exact distributions provided by user request
const REEL1: [&str; 20] = [
    "🍒","🍊","🍋","🔔","🍒","🍇","🟦","🍊","🍒","🔔",
    "🍇","🍊","🍋","7️⃣","🍒","🔔","🍇","🍊","🍋","🍒",
];
const REEL2: [&str; 20] = [
    "🍋","🍊","🔔","🍒","🍇","🍋","🍊","🔔","🍇","🟦",
    "🍋","7️⃣","🍊","🔔","🍇","🍋","🔔","🍊","🍒","🍋",
];
const REEL3: [&str; 20] = [
    "🍊","🍋","🍒","🔔","🍋","🍊","🍇","🔔","🍋","7️⃣",
    "🍊","🍒","🔔","🍋","🟦","🍒","🍋","🔔","🍊","🍋",
];

/// Persistent state tracked per player (Meshtastic node ID)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    pub coins: u32,
    pub last_reset: DateTime<Utc>,
    #[serde(default)]
    pub total_spins: u32,
    #[serde(default)]
    pub total_wins: u32,
    #[serde(default)]
    pub jackpots: u32,
    #[serde(default)]
    pub last_spin: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_jackpot: Option<DateTime<Utc>>,
}

/// On‑disk file schema for all players. Stored at `<data_dir>/slotmachine/players.json`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PlayersFile {
    pub players: HashMap<String, PlayerState>,
}

fn ensure_dir(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

fn players_file_path(base_dir: &str) -> PathBuf {
    Path::new(base_dir).join("slotmachine").join("players.json")
}

fn jackpot_file_path(base_dir: &str) -> PathBuf {
    Path::new(base_dir).join("slotmachine").join("jackpot.json")
}

fn load_players(base_dir: &str) -> PlayersFile {
    let dir = Path::new(base_dir).join("slotmachine");
    if let Err(e) = ensure_dir(&dir) {
        log::warn!("slotmachine: unable to ensure dir {:?}: {}", dir, e);
    }
    let path = players_file_path(base_dir);
    if let Ok(mut f) = fs::OpenOptions::new().read(true).open(&path) {
        // Try shared lock for read
        let _ = f.lock_shared();
        let mut s = String::new();
        if let Err(e) = f.read_to_string(&mut s) {
            log::warn!("slotmachine: failed reading players.json: {}", e);
            return PlayersFile::default();
        }
        serde_json::from_str(&s).unwrap_or_default()
    } else {
        PlayersFile::default()
    }
}

fn save_players(base_dir: &str, players: &PlayersFile) {
    let dir = Path::new(base_dir).join("slotmachine");
    if let Err(e) = ensure_dir(&dir) {
        log::warn!("slotmachine: unable to ensure dir {:?}: {}", dir, e);
        return;
    }
    let path = players_file_path(base_dir);
    match serde_json::to_string_pretty(players) {
        Ok(data) => {
            if let Ok(mut f) = fs::OpenOptions::new().create(true).write(true).truncate(true).open(&path) {
                if f.lock_exclusive().is_ok() {
                    let _ = f.write_all(data.as_bytes());
                    let _ = f.flush();
                    let _ = f.unlock();
                }
            }
        }
        Err(e) => log::warn!("slotmachine: serialize error: {}", e),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct GlobalJackpot {
    /// Count of losing spins across all players since the last jackpot payout
    #[serde(default)]
    losses: u64,
    /// Time of last jackpot payout
    #[serde(default)]
    last_win: Option<DateTime<Utc>>,
    /// Node ID string of last jackpot winner (Meshtastic node id as string)
    #[serde(default)]
    last_win_node: Option<String>,
}

// load/save helpers removed in favor of atomic read-modify-write functions below

fn jackpot_payout_and_reset(base_dir: &str, now: DateTime<Utc>, winner: &str) -> u32 {
    let dir = Path::new(base_dir).join("slotmachine");
    let _ = ensure_dir(&dir);
    let path = jackpot_file_path(base_dir);
    // Open with read/write to allow locking and persistence
    let mut jackpot = GlobalJackpot::default();
    if let Ok(mut f) = fs::OpenOptions::new().create(true).read(true).write(true).truncate(false).open(&path) {
        let _ = f.lock_exclusive();
        // Read current
        let mut s = String::new();
        let _ = f.read_to_string(&mut s);
        if !s.is_empty() {
            jackpot = serde_json::from_str(&s).unwrap_or_default();
        }
    // Compute payout: 500 base + losses * BET_COINS
    let mut payout_u64 = 500u64 + jackpot.losses.saturating_mul(BET_COINS as u64);
        if payout_u64 > u32::MAX as u64 { payout_u64 = u32::MAX as u64; }
        let payout = payout_u64 as u32;
    // Reset and save
        jackpot.losses = 0;
        jackpot.last_win = Some(now);
    jackpot.last_win_node = Some(winner.to_string());
        let data = serde_json::to_string_pretty(&jackpot).unwrap_or_else(|_| "{}".to_string());
        let _ = f.set_len(0);
        let _ = f.write_all(data.as_bytes());
        let _ = f.flush();
        let _ = f.unlock();
        payout
    } else {
        // File open failed, still honor minimum jackpot
        500
    }
}

fn jackpot_record_loss(base_dir: &str) {
    let dir = Path::new(base_dir).join("slotmachine");
    let _ = ensure_dir(&dir);
    let path = jackpot_file_path(base_dir);
    if let Ok(mut f) = fs::OpenOptions::new().create(true).read(true).write(true).truncate(false).open(&path) {
        let _ = f.lock_exclusive();
        let mut s = String::new();
        let _ = f.read_to_string(&mut s);
        let mut jackpot: GlobalJackpot = if s.is_empty() { GlobalJackpot::default() } else { serde_json::from_str(&s).unwrap_or_default() };
        jackpot.losses = jackpot.losses.saturating_add(1);
        let data = serde_json::to_string_pretty(&jackpot).unwrap_or_else(|_| "{}".to_string());
        let _ = f.set_len(0);
        let _ = f.write_all(data.as_bytes());
        let _ = f.flush();
        let _ = f.unlock();
    }
}

#[derive(Debug, Clone)]
pub struct JackpotSummary {
    /// Current jackpot amount in coins if won now (max(losses, 500))
    pub amount: u64,
    /// Date of last jackpot payout (UTC, date only)
    pub last_win_date: Option<chrono::NaiveDate>,
    /// Node ID of last winner (string), if known
    pub last_win_node: Option<String>,
}

/// Read the current global jackpot summary. This uses a shared lock for consistency.
pub fn get_jackpot_summary(base_dir: &str) -> JackpotSummary {
    let dir = Path::new(base_dir).join("slotmachine");
    let _ = ensure_dir(&dir);
    let path = jackpot_file_path(base_dir);
    let mut jackpot: GlobalJackpot = GlobalJackpot::default();
    if let Ok(mut f) = fs::OpenOptions::new().read(true).open(&path) {
        let _ = f.lock_shared();
        let mut s = String::new();
        let _ = f.read_to_string(&mut s);
        if !s.is_empty() {
            jackpot = serde_json::from_str(&s).unwrap_or_default();
        }
        let _ = f.unlock();
    }
    let amt = 500u64 + jackpot.losses.saturating_mul(BET_COINS as u64);
    JackpotSummary {
        amount: amt,
        last_win_date: jackpot.last_win.map(|t| t.date_naive()),
        last_win_node: jackpot.last_win_node,
    }
}

/// Result of a spin for UI/formatting by the caller.
#[derive(Debug, Clone)]
pub struct SpinOutcome {
    pub r1: &'static str,
    pub r2: &'static str,
    pub r3: &'static str,
    pub multiplier: u32,
    pub winnings: u32,
    pub description: String,
}

fn spin_reel<const N: usize>(reel: &[&'static str; N]) -> &'static str {
    let mut rng = rand::thread_rng();
    let idx = rng.gen_range(0..N);
    reel[idx]
}

/// Evaluate three symbols and return the payout multiplier and a human description.
fn evaluate(r1: &str, r2: &str, r3: &str) -> (u32, String) {
    // Triple matches first
    if r1 == r2 && r2 == r3 {
        let mult = match r1 {
            "7️⃣" => 100,
            "🟦" => 50,
            "🔔" => 20,
            "🍇" => 14,
            "🍊" => 10,
            "🍋" => 8,
            "🍒" => 5,
            _ => 0,
        };
        let desc = if mult == 100 { "JACKPOT! 7️⃣7️⃣7️⃣".to_string() }
                   else { format!("Triple {}", r1) };
        return (mult, desc);
    }
    // Cherry pays by count
    let cherries = [r1, r2, r3].iter().filter(|&&sym| sym == "🍒").count() as u32;
    if cherries == 2 { return (3, "Two cherries".into()); }
    if cherries == 1 { return (2, "Cherry".into()); }
    (0, "No win".into())
}

/// Perform a single spin for `player_id`.
///
/// Contract:
/// - Input: `base_dir` is the configured storage base dir; `player_id` is a stable node ID
/// - Side effects: updates `<base_dir>/slotmachine/players.json` with coin balance and stats
/// - Behavior: deducts [`BET_COINS`], spins reels, applies payout, updates stats
/// - Refill: if balance is 0 and `REFILL_HOURS` elapsed since `last_reset`, grants [`DAILY_GRANT`]
/// - Returns: `(SpinOutcome, balance_after)`; if unable to afford, `r1=r2=r3="⛔"` and no changes
pub fn perform_spin(base_dir: &str, player_id: &str) -> (SpinOutcome, u32) {
    // Load players
    let mut file = load_players(base_dir);
    let now = Utc::now();

    // Compute outcome within a limited scope to avoid borrow conflicts
    let (outcome, balance_after) = {
        let entry = file
            .players
            .entry(player_id.to_string())
            .or_insert(PlayerState { coins: DAILY_GRANT, last_reset: now, total_spins: 0, total_wins: 0, jackpots: 0, last_spin: None, last_jackpot: None });

        // Handle zero-balance refill window
        if entry.coins < BET_COINS && entry.coins == 0 {
            let elapsed = now.signed_duration_since(entry.last_reset);
            if elapsed >= ChronoDuration::hours(REFILL_HOURS) {
                entry.coins = DAILY_GRANT;
                entry.last_reset = now;
            }
        }

        // If still can't afford, return a special outcome with no spin
        if entry.coins < BET_COINS {
            let remaining = ChronoDuration::hours(REFILL_HOURS)
                - now.signed_duration_since(entry.last_reset);
            let hours = remaining.num_hours().max(0);
            let mins = (remaining.num_minutes().max(0)) % 60;
            let desc = format!("Out of coins. Next refill in ~{}h {}m", hours, mins);
            let outcome = SpinOutcome {
                r1: "⛔",
                r2: "⛔",
                r3: "⛔",
                multiplier: 0,
                winnings: 0,
                description: desc,
            };
            (outcome, entry.coins)
        } else {
            // Deduct bet
            entry.coins = entry.coins.saturating_sub(BET_COINS);

            // Spin
            let r1 = spin_reel(&REEL1);
            let r2 = spin_reel(&REEL2);
            let r3 = spin_reel(&REEL3);
            let (mult, desc) = evaluate(r1, r2, r3);
            let winnings: u32;
            if mult == 100 {
                // Jackpot payout: number of losses (coins) with a floor of 500 coins, atomically reset
                winnings = jackpot_payout_and_reset(base_dir, now, player_id);
                entry.coins = entry.coins.saturating_add(winnings);
            } else {
                winnings = BET_COINS.saturating_mul(mult);
                entry.coins = entry.coins.saturating_add(winnings);
                // Accumulate pot on losses only (multiplier == 0)
                if mult == 0 {
                    jackpot_record_loss(base_dir);
                }
            }
            // Stats
            entry.total_spins = entry.total_spins.saturating_add(1);
            entry.last_spin = Some(now);
            if mult > 0 { entry.total_wins = entry.total_wins.saturating_add(1); }
            if mult == 100 {
                entry.jackpots = entry.jackpots.saturating_add(1);
                entry.last_jackpot = Some(now);
            }
            let bal = entry.coins;
            (
                SpinOutcome { r1, r2, r3, multiplier: mult, winnings, description: desc },
                bal,
            )
        }
    };

    // Persist after mutation
    save_players(base_dir, &file);
    // Jackpot state already updated atomically above if needed

    (outcome, balance_after)
}

/// If `player_id` is out of coins, return `(hours, minutes)` until the next daily refill.
/// Returns `None` if the player has coins or does not exist.
pub fn next_refill_eta(base_dir: &str, player_id: &str) -> Option<(i64, i64)> {
    let file = load_players(base_dir);
    let entry = file.players.get(player_id)?;
    if entry.coins > 0 { return None; }
    let now = Utc::now();
    let remaining = ChronoDuration::hours(REFILL_HOURS) - now.signed_duration_since(entry.last_reset);
    if remaining <= ChronoDuration::zero() { Some((0,0)) } else { Some((remaining.num_hours(), (remaining.num_minutes() % 60))) }
}

/// Public summary used by `^SLOTSTATS` to report a user's stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSummary {
    pub coins: u32,
    pub total_spins: u32,
    pub total_wins: u32,
    pub jackpots: u32,
    pub last_spin: Option<DateTime<Utc>>,
    pub last_jackpot: Option<DateTime<Utc>>,
}

/// Load and return the `PlayerSummary` for `player_id`, or `None` if no record exists.
pub fn get_player_summary(base_dir: &str, player_id: &str) -> Option<PlayerSummary> {
    let file = load_players(base_dir);
    let p = file.players.get(player_id)?;
    Some(PlayerSummary {
        coins: p.coins,
        total_spins: p.total_spins,
        total_wins: p.total_wins,
        jackpots: p.jackpots,
        last_spin: p.last_spin,
        last_jackpot: p.last_jackpot,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use tempfile::tempdir;
    use std::fs;

    fn write_players(base: &str, players: &PlayersFile) {
        let dir = Path::new(base).join("slotmachine");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("players.json");
        let data = serde_json::to_string_pretty(players).unwrap();
        std::fs::write(path, data).unwrap();
    }

    #[test]
    fn out_of_coins_blocks_spin() {
        let tmp = tempdir().unwrap();
        let base = tmp.path().to_str().unwrap();
        let mut file = PlayersFile::default();
        file.players.insert(
            "node1".to_string(),
            PlayerState { coins: 0, last_reset: Utc::now(), total_spins: 0, total_wins: 0, jackpots: 0, last_spin: None, last_jackpot: None }
        );
        write_players(base, &file);
        let (out, bal) = perform_spin(base, "node1");
        assert_eq!(out.r1, "⛔");
        assert_eq!(bal, 0);
        assert!(out.description.contains("Out of coins"));
    }

    #[test]
    fn refill_after_24h_allows_spin() {
        let tmp = tempdir().unwrap();
        let base = tmp.path().to_str().unwrap();
        let mut file = PlayersFile::default();
        file.players.insert(
            "node2".to_string(),
            PlayerState { coins: 0, last_reset: Utc::now() - Duration::hours(REFILL_HOURS + 1), total_spins: 0, total_wins: 0, jackpots: 0, last_spin: None, last_jackpot: None }
        );
        write_players(base, &file);
        let (_out, bal) = perform_spin(base, "node2");
        // After refill and one spin, balance should be at least DAILY_GRANT - BET
        assert!(bal >= DAILY_GRANT - BET_COINS);
    // Upper bound for fresh state: jackpot minimum equals BET*100 (500 coins). Pot can be larger over time.
    assert!(bal <= DAILY_GRANT - BET_COINS + BET_COINS * 100);
    }
}
