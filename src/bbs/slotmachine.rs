use chrono::{DateTime, Utc, Duration as ChronoDuration};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use fs2::FileExt;
use std::path::{Path, PathBuf};

/// Fixed bet cost per spin
pub const BET_COINS: u32 = 5;
/// New player grant and daily refill amount
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    pub coins: u32,
    pub last_reset: DateTime<Utc>,
}

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

pub fn perform_spin(base_dir: &str, player_id: &str) -> (SpinOutcome, u32) {
    // Load players
    let mut file = load_players(base_dir);
    let now = Utc::now();

    // Compute outcome within a limited scope to avoid borrow conflicts
    let (outcome, balance_after) = {
        let entry = file
            .players
            .entry(player_id.to_string())
            .or_insert(PlayerState { coins: DAILY_GRANT, last_reset: now });

        // Handle zero-balance refill window
        if entry.coins < BET_COINS {
            if entry.coins == 0 {
                let elapsed = now.signed_duration_since(entry.last_reset);
                if elapsed >= ChronoDuration::hours(REFILL_HOURS) {
                    entry.coins = DAILY_GRANT;
                    entry.last_reset = now;
                }
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
            let winnings = BET_COINS * mult;
            entry.coins = entry.coins.saturating_add(winnings);
            let bal = entry.coins;
            (
                SpinOutcome { r1, r2, r3, multiplier: mult, winnings, description: desc },
                bal,
            )
        }
    };

    // Persist after mutation
    save_players(base_dir, &file);

    (outcome, balance_after)
}

pub fn next_refill_eta(base_dir: &str, player_id: &str) -> Option<(i64, i64)> {
    let file = load_players(base_dir);
    let entry = file.players.get(player_id)?;
    if entry.coins > 0 { return None; }
    let now = Utc::now();
    let remaining = ChronoDuration::hours(REFILL_HOURS) - now.signed_duration_since(entry.last_reset);
    if remaining <= ChronoDuration::zero() { Some((0,0)) } else { Some((remaining.num_hours(), (remaining.num_minutes() % 60))) }
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
            PlayerState { coins: 0, last_reset: Utc::now() }
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
            PlayerState { coins: 0, last_reset: Utc::now() - Duration::hours(REFILL_HOURS + 1) }
        );
        write_players(base, &file);
        let (_out, bal) = perform_spin(base, "node2");
        // After refill and one spin, balance should be at least DAILY_GRANT - BET
        assert!(bal >= DAILY_GRANT - BET_COINS);
        // Upper bound: won jackpot => +BET*100
        assert!(bal <= DAILY_GRANT - BET_COINS + BET_COINS * 100);
    }
}
