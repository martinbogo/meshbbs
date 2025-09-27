//! Unix fortune cookie mini-feature used by public channel command ^FORTUNE.
//!
//! This module provides a stateless fortune cookie system inspired by the classic Unix
//! `fortune` command. It contains a curated database of 140 wisdom quotes, programming
//! humor, philosophical insights, and motivational messages.
//!
//! # Behavior
//!
//! - **Stateless**: No persistence required; pure function calls
//! - **Delivery**: Public broadcast only (best-effort), same reliability posture as ^SLOT
//! - **Rate limiting**: Handled by `PublicState.allow_fortune` (5-second per-node cooldown)
//! - **Mesh-optimized**: All entries under 200 characters for efficient transmission
//!
//! # Fortune Database
//!
//! The fortune database is compiled from public domain sources including:
//! - Classic BSD Unix fortune collections
//! - Programming and technology wisdom
//! - Literature and philosophical quotes
//! - Clean, family-friendly humor
//! - Motivational and inspirational content
//!
//! # Usage
//!
//! Users send `^FORTUNE` on the public channel to receive a random fortune:
//!
//! ```text
//! User: ^FORTUNE
//! BBS:  ^FORTUNE ⟶ The only true wisdom is in knowing you know nothing. — Socrates
//! ```
//!
//! # Thread Safety
//!
//! All functions in this module are thread-safe and can be called concurrently
//! from multiple tasks without synchronization.

use rand::Rng;

/// Curated collection of fortune cookies from classic Unix databases.
/// Mix of wisdom, literature, programming quotes, and clean humor.
/// All entries under 200 characters for mesh network compatibility.
const FORTUNES: [&str; 140] = [
    // Classic wisdom
    "The only true wisdom is in knowing you know nothing. — Socrates",
    "In the middle of difficulty lies opportunity. — Albert Einstein",
    "It is during our darkest moments that we must focus to see the light. — Aristotle",
    "The journey of a thousand miles begins with one step. — Lao Tzu",
    "Yesterday is history, tomorrow is a mystery, today is a gift. — Eleanor Roosevelt",
    "Be yourself; everyone else is already taken. — Oscar Wilde",
    "Two things are infinite: the universe and human stupidity; I'm not sure about the universe. — Einstein",
    "The only way to do great work is to love what you do. — Steve Jobs",
    "Life is what happens when you're busy making other plans. — John Lennon",
    "The future belongs to those who believe in the beauty of their dreams. — Eleanor Roosevelt",
    
    // Programming & Tech
    "There are only two hard things in Computer Science: cache invalidation and naming things. — Phil Karlton",
    "The best way to get a project done faster is to start sooner. — Jim Highsmith",
    "Code is like humor. When you have to explain it, it's bad. — Cory House",
    "First, solve the problem. Then, write the code. — John Johnson",
    "Any fool can write code that a computer can understand. Good programmers write code humans understand. — Martin Fowler",
    "The most important property of a program is whether it accomplishes the intention of its user. — C.A.R. Hoare",
    "Programs must be written for people to read, and only incidentally for machines to execute. — Abelson & Sussman",
    "The best error message is the one that never shows up. — Thomas Fuchs",
    "Debugging is twice as hard as writing the code in the first place. — Brian Kernighan",
    "Talk is cheap. Show me the code. — Linus Torvalds",
    "The computer was born to solve problems that did not exist before. — Bill Gates",
    "Software is a great combination between artistry and engineering. — Bill Gates",
    "It's not a bug – it's an undocumented feature. — Anonymous",
    "There's nothing more permanent than a temporary hack. — Kyle Simpson",
    "Good code is its own best documentation. — Steve McConnell",
    
    // Science & Discovery
    "Science is a way of thinking much more than it is a body of knowledge. — Carl Sagan",
    "The important thing is not to stop questioning. — Albert Einstein",
    "Somewhere, something incredible is waiting to be known. — Carl Sagan",
    "The greatest enemy of knowledge is not ignorance, it is the illusion of knowledge. — Stephen Hawking",
    "What we know is a drop, what we don't know is an ocean. — Isaac Newton",
    "I have not failed. I've just found 10,000 ways that won't work. — Thomas Edison",
    "Research is what I'm doing when I don't know what I'm doing. — Wernher von Braun",
    "The good thing about science is that it's true whether or not you believe in it. — Neil deGrasse Tyson",
    
    // Literature & Philosophy  
    "Not all those who wander are lost. — J.R.R. Tolkien",
    "It was the best of times, it was the worst of times. — Charles Dickens",
    "To be or not to be, that is the question. — William Shakespeare",
    "All that is gold does not glitter. — J.R.R. Tolkien",
    "The pen is mightier than the sword. — Edward Bulwer-Lytton",
    "I think, therefore I am. — René Descartes",
    "The unexamined life is not worth living. — Socrates",
    "Man is condemned to be free. — Jean-Paul Sartre",
    "Hell is other people. — Jean-Paul Sartre",
    "The only constant in life is change. — Heraclitus",
    
    // Clean Humor & Wit
    "I'm not arguing, I'm just explaining why I'm right. — Anonymous",
    "The early bird might get the worm, but the second mouse gets the cheese. — Anonymous",
    "If at first you don't succeed, then skydiving definitely isn't for you. — Steven Wright",
    "I told my wife she was drawing her eyebrows too high. She looked surprised. — Anonymous",
    "Why don't scientists trust atoms? Because they make up everything! — Anonymous",
    "Parallel lines have so much in common. It's a shame they'll never meet. — Anonymous",
    "I'm reading a book about anti-gravity. It's impossible to put down! — Anonymous",
    "Did you hear about the mathematician who's afraid of negative numbers? He stops at nothing! — Anonymous",
    
    // Motivational
    "Success is not final, failure is not fatal: it is the courage to continue that counts. — Churchill",
    "The only impossible journey is the one you never begin. — Tony Robbins",
    "Your limitation—it's only your imagination. — Anonymous",
    "Push yourself, because no one else is going to do it for you. — Anonymous",
    "Great things never come from comfort zones. — Anonymous",
    "Dream it. Wish it. Do it. — Anonymous",
    "Success doesn't just find you. You have to go out and get it. — Anonymous",
    "The harder you work for something, the greater you'll feel when you achieve it. — Anonymous",
    "Don't stop when you're tired. Stop when you're done. — Anonymous",
    "Wake up with determination. Go to bed with satisfaction. — Anonymous",
    
    // Technology & Future
    "The advance of technology is based on making it fit in so you don't really notice it. — Bill Gates",
    "Technology is best when it brings people together. — Matt Mullenweg",
    "The Internet is becoming the town square for the global village of tomorrow. — Bill Gates",
    "Innovation distinguishes between a leader and a follower. — Steve Jobs",
    "The real problem is not whether machines think but whether men do. — B.F. Skinner",
    "Any sufficiently advanced technology is indistinguishable from magic. — Arthur C. Clarke",
    "The future is not some place we are going, but one we are creating. — John Schaar",
    
    // Unix/Computing Culture
    "Unix is simple. It just takes a genius to understand its simplicity. — Dennis Ritchie",
    "UNIX is basically a simple operating system, but you have to be a genius to understand the simplicity. — Dennis Ritchie",
    "The power of Unix lies in the philosophy behind it. — Anonymous",
    "Everything is a file. — Unix Philosophy",
    "Write programs that do one thing and do it well. — Unix Philosophy",
    "Worse is better. — Richard Gabriel",
    "When in doubt, use brute force. — Ken Thompson",
    
    // Classic Sayings
    "A penny saved is a penny earned. — Benjamin Franklin",
    "Actions speak louder than words. — Abraham Lincoln",
    "Fortune favors the bold. — Latin Proverb",
    "Knowledge is power. — Francis Bacon",
    "Time is money. — Benjamin Franklin",
    "Practice makes perfect. — Ancient Proverb",
    "Where there's a will, there's a way. — English Proverb",
    "You can't judge a book by its cover. — English Proverb",
    "The squeaky wheel gets the grease. — American Proverb",
    "Better late than never. — English Proverb",
    "Don't count your chickens before they hatch. — Aesop",
    "Every cloud has a silver lining. — English Proverb",
    "Rome wasn't built in a day. — Medieval French Proverb",
    "When life gives you lemons, make lemonade. — Elbert Hubbard",
    "The grass is always greener on the other side. — English Proverb",
    
    // Math & Logic
    "Mathematics is the language with which God has written the universe. — Galileo",
    "In mathematics you don't understand things. You just get used to them. — John von Neumann",
    "Pure mathematics is, in its way, the poetry of logical ideas. — Albert Einstein",
    "God does not play dice with the universe. — Albert Einstein",
    "Mathematics is not about numbers, equations, computations, or algorithms: it is about understanding. — William Paul Thurston",
    
    // Short Observations
    "Reality is that which, when you stop believing in it, doesn't go away. — Philip K. Dick",
    "The only way to make sense out of change is to plunge into it, move with it, and join the dance. — Alan Watts",
    "We are what we repeatedly do. Excellence, then, is not an act, but a habit. — Aristotle",
    "The best time to plant a tree was 20 years ago. The second best time is now. — Chinese Proverb",
    "A goal without a plan is just a wish. — Antoine de Saint-Exupéry",
    "You miss 100% of the shots you don't take. — Wayne Gretzky",
    "Whether you think you can or you think you can't, you're right. — Henry Ford",
    "It does not matter how slowly you go as long as you do not stop. — Confucius",
    "Everything you've ever wanted is on the other side of fear. — George Addair",
    "Believe you can and you're halfway there. — Theodore Roosevelt",
    "The only person you are destined to become is the person you decide to be. — Ralph Waldo Emerson",
    "Go confidently in the direction of your dreams. Live the life you have imagined. — Henry David Thoreau",
    "Few things can help an individual more than to place responsibility on him. — Booker T. Washington",
    "It is never too late to be what you might have been. — George Eliot",
    "Life is 10% what happens to you and 90% how you react to it. — Charles R. Swindoll",
    
    // Technology Humor
    "There are 10 types of people in the world: those who understand binary and those who don't. — Anonymous",
    "To understand recursion, you must first understand recursion. — Anonymous",
    "It works on my machine. — Every Developer Ever",
    "Have you tried turning it off and on again? — IT Support Everywhere",
    "99 little bugs in the code, 99 little bugs. Take one down, patch it around, 117 little bugs in the code. — Anonymous",
    "Programming is like sex: one mistake and you have to support it for the rest of your life. — Michael Sinz",
    "If debugging is the process of removing bugs, then programming must be the process of putting them in. — Edsger Dijkstra",
    "Measuring programming progress by lines of code is like measuring aircraft building progress by weight. — Bill Gates",
    "The best thing about a boolean is even if you are wrong, you are only off by a bit. — Anonymous",
    "A user interface is like a joke. If you have to explain it, it's not that good. — Martin LeBlanc",
    
    // Final Wisdom
    "The only true failure is the failure to try. — Anonymous",
    "Don't wait for opportunity. Create it. — Anonymous",
    "Life begins at the end of your comfort zone. — Neale Donald Walsch",
    "The difference between ordinary and extraordinary is that little extra. — Jimmy Johnson",
    "Champions don't show up to get everything they want; they show up to give everything they have. — Anonymous",
    "Success is walking from failure to failure with no loss of enthusiasm. — Winston Churchill",
    "The expert in anything was once a beginner. — Helen Hayes",
    "Don't let yesterday take up too much of today. — Will Rogers",
    "You learn more from failure than from success. Don't let it stop you. Failure builds character. — Anonymous",
    "If you are not willing to risk the usual, you will have to settle for the ordinary. — Jim Rohn",
    "Take up one idea. Make that one idea your life. Think of it, dream of it, live on that idea. — Swami Vivekananda",
    "All our dreams can come true if we have the courage to pursue them. — Walt Disney",
    "Good things come to people who wait, but better things come to those who go out and get them. — Anonymous",
    "If you do what you always did, you will get what you always got. — Anonymous",
    "Happiness is not something readymade. It comes from your own actions. — Dalai Lama",
    "The way to get started is to quit talking and begin doing. — Walt Disney",
    "Don't let the fear of losing be greater than the excitement of winning. — Robert Kiyosaki",
    "If you want to lift yourself up, lift up someone else. — Booker T. Washington",
    "Success is not how high you have climbed, but how you make a positive difference. — Roy T. Bennett",
    "What lies behind us and what lies before us are tiny matters compared to what lies within us. — Ralph Waldo Emerson",
];

/// Pick a random fortune from the classic database.
/// 
/// Returns a reference to a static string containing a fortune cookie message.
/// All fortunes are guaranteed to be under 200 characters for mesh network compatibility.
/// 
/// # Examples
/// 
/// ```
/// use meshbbs::bbs::fortune::get_fortune;
/// 
/// let fortune = get_fortune();
/// assert!(!fortune.is_empty());
/// assert!(fortune.len() <= 200);
/// ```
/// 
/// # Thread Safety
/// 
/// This function is thread-safe and uses `rand::thread_rng()` for randomization.
/// Multiple calls from different threads will produce independent random results.
pub fn get_fortune() -> &'static str {
    let mut rng = rand::thread_rng();
    let idx = rng.gen_range(0..FORTUNES.len());
    FORTUNES[idx]
}

/// Get the total number of fortunes in the database.
/// 
/// This function is primarily useful for testing and diagnostics.
/// 
/// # Examples
/// 
/// ```
/// use meshbbs::bbs::fortune::fortune_count;
/// 
/// assert_eq!(fortune_count(), 140);
/// ```
pub fn fortune_count() -> usize {
    FORTUNES.len()
}

/// Get the maximum length of any fortune in the database.
/// 
/// This function is useful for validation and ensuring all fortunes
/// meet the mesh network size constraints.
/// 
/// # Examples
/// 
/// ```
/// use meshbbs::bbs::fortune::max_fortune_length;
/// 
/// assert!(max_fortune_length() <= 200);
/// ```
pub fn max_fortune_length() -> usize {
    FORTUNES.iter().map(|f| f.len()).max().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fortunes_count_140() {
        assert_eq!(FORTUNES.len(), 140);
    }

    #[test]
    fn all_fortunes_under_200_chars() {
        for (i, fortune) in FORTUNES.iter().enumerate() {
            assert!(
                fortune.len() <= 200,
                "Fortune {} is too long ({} chars): {}",
                i, fortune.len(), fortune
            );
        }
    }

    #[test]
    fn all_fortunes_non_empty() {
        for (i, fortune) in FORTUNES.iter().enumerate() {
            assert!(
                !fortune.is_empty(),
                "Fortune {} is empty",
                i
            );
        }
    }

    #[test]
    fn all_fortunes_contain_printable_chars() {
        for (i, fortune) in FORTUNES.iter().enumerate() {
            assert!(
                fortune.chars().all(|c| !c.is_control() || c.is_ascii_whitespace()),
                "Fortune {} contains control characters: {}",
                i, fortune
            );
        }
    }

    #[test]
    fn fortune_returns_valid_response() {
        let fortune = get_fortune();
        assert!(!fortune.is_empty());
        assert!(fortune.len() <= 200);
        assert!(FORTUNES.contains(&fortune));
    }

    #[test]
    fn fortune_randomness_check() {
        // Run multiple times to ensure we get different results
        let mut results = std::collections::HashSet::new();
        for _ in 0..50 {
            results.insert(get_fortune());
        }
        // Should get at least 10 different fortunes in 50 tries
        assert!(results.len() >= 10, "Fortune randomness seems poor: only {} unique results", results.len());
    }

    #[test]
    fn fortune_thread_safety_simulation() {
        // Simulate concurrent access by calling get_fortune many times rapidly
        let mut handles = vec![];
        
        for _ in 0..10 {
            let handle = std::thread::spawn(|| {
                let mut local_results = std::collections::HashSet::new();
                for _ in 0..20 {
                    local_results.insert(get_fortune());
                }
                local_results
            });
            handles.push(handle);
        }
        
        let mut all_results = std::collections::HashSet::new();
        for handle in handles {
            let thread_results = handle.join().unwrap();
            all_results.extend(thread_results);
        }
        
        // Should collect a good variety of fortunes across threads
        assert!(all_results.len() >= 15, "Concurrent access produced only {} unique fortunes", all_results.len());
    }

    #[test]
    fn fortune_database_quality_checks() {
        // Check that we have a good mix of different types of content
        let programming_related = FORTUNES.iter().filter(|f| {
            f.to_lowercase().contains("code") || 
            f.to_lowercase().contains("program") ||
            f.to_lowercase().contains("computer") ||
            f.to_lowercase().contains("software")
        }).count();
        
        let philosophical = FORTUNES.iter().filter(|f| {
            f.contains("Socrates") || 
            f.contains("Aristotle") ||
            f.contains("Einstein") ||
            f.contains("wisdom")
        }).count();
        
        // Ensure we have a reasonable distribution
        assert!(programming_related >= 10, "Should have at least 10 programming-related fortunes, found {}", programming_related);
        assert!(philosophical >= 5, "Should have at least 5 philosophical fortunes, found {}", philosophical);
    }

    #[test]
    fn fortune_count_matches_array() {
        assert_eq!(fortune_count(), FORTUNES.len());
        assert_eq!(fortune_count(), 140);
    }

    #[test]
    fn max_fortune_length_validation() {
        let max_len = max_fortune_length();
        assert!(max_len <= 200, "Maximum fortune length {} exceeds 200 character limit", max_len);
        assert!(max_len > 0, "Maximum fortune length should be greater than 0");
        
        // Verify it actually matches the longest fortune
        let actual_max = FORTUNES.iter().map(|f| f.len()).max().unwrap();
        assert_eq!(max_len, actual_max);
    }

    #[test]
    fn helper_functions_consistency() {
        // Ensure helper functions are consistent with the actual data
        assert_eq!(fortune_count(), FORTUNES.len());
        
        let calculated_max = FORTUNES.iter().map(|f| f.len()).max().unwrap_or(0);
        assert_eq!(max_fortune_length(), calculated_max);
    }
}