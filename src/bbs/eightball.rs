//! Magic 8-Ball mini-feature used by public channel command ^8BALL.
//!
//! Behavior:
//! - Stateless: no persistence; just returns a random classic response
//! - Delivery: public broadcast only (best-effort), same reliability posture as ^SLOT
//! - Rate limit: handled by PublicState.allow_8ball (light per-node cooldown like ^SLOT)

use rand::Rng;

/// Classic 20 Magic 8-Ball responses.
const RESPONSES: [&str; 20] = [
    // Positive
    "It is certain.",
    "It is decidedly so.",
    "Without a doubt.",
    "Yes — definitely.",
    "You may rely on it.",
    "As I see it, yes.",
    "Most likely.",
    "Outlook good.",
    "Yes.",
    "Signs point to yes.",
    // Neutral
    "Reply hazy, try again.",
    "Ask again later.",
    "Better not tell you now.",
    "Cannot predict now.",
    "Concentrate and ask again.",
    // Negative
    "Don't count on it.",
    "My reply is no.",
    "My sources say no.",
    "Outlook not so good.",
    "Very doubtful.",
];

/// Pick a random Magic 8-Ball response.
pub fn ask() -> &'static str {
    let mut rng = rand::thread_rng();
    let idx = rng.gen_range(0..RESPONSES.len());
    RESPONSES[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_count_20() {
        assert_eq!(super::RESPONSES.len(), 20);
    }

    #[test]
    fn ask_returns_known_response() {
        let resp = ask();
        assert!(super::RESPONSES.contains(&resp));
    }
}
