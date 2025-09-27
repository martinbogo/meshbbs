/// Test the chunking logic for HELP messages to ensure they stay under 230 bytes
#[test]
fn help_chunk_size_validation() {
    // Simulate the commands that would be included
    let commands = vec![
        "^HELP - Show this help",
        "^LOGIN <user> - Register for BBS",
        "^WEATHER - Current conditions", 
        "^SLOT - Play slot machine",
        "^SLOTSTATS - Show your stats",
        "^8BALL - Magic 8-Ball oracle",
        "^FORTUNE - Random wisdom",
    ];
    
    // Test with a long BBS name and user to simulate worst case
    let bbs_name = "Test BBS Station With A Very Long Name That Could Cause Issues";
    let user_name = "VeryLongUserName123";
    
    // Test chunking function (copied from server logic)
    let chunks = create_help_chunks(bbs_name, user_name, &commands);
    
    // Verify all chunks are under 230 bytes
    for (i, chunk) in chunks.iter().enumerate() {
        println!("Chunk {}: {} bytes - '{}'", i + 1, chunk.len(), chunk);
        assert!(chunk.len() <= 230, "Chunk {} is {} bytes, exceeds 230 byte limit: '{}'", i + 1, chunk.len(), chunk);
        assert!(chunk.len() > 0, "Chunk {} is empty", i + 1);
    }
    
    // Should have multiple chunks due to length
    assert!(chunks.len() >= 2, "Expected at least 2 chunks for long content, got {}", chunks.len());
    
    // First chunk should contain the main header and some commands
    assert!(chunks[0].contains("Public Commands (for"));
    assert!(chunks[0].contains("DM for BBS access"));
    
    // Subsequent chunks should use "More:" header
    if chunks.len() > 1 {
        assert!(chunks[1].contains("More:"));
        assert!(!chunks[1].contains("DM for BBS access"));
    }
}

/// Test with typical BBS name length
#[test]
fn help_chunk_typical_length() {
    let commands = vec![
        "^HELP - Show this help",
        "^LOGIN <user> - Register for BBS",
        "^WEATHER - Current conditions",
        "^SLOT - Play slot machine", 
        "^SLOTSTATS - Show your stats",
        "^8BALL - Magic 8-Ball oracle",
        "^FORTUNE - Random wisdom",
    ];
    
    let bbs_name = "meshbbs Station";
    let user_name = "WAP2";
    
    let chunks = create_help_chunks(bbs_name, user_name, &commands);
    
    // Verify chunks
    for (i, chunk) in chunks.iter().enumerate() {
        println!("Typical case chunk {}: {} bytes", i + 1, chunk.len());
        assert!(chunk.len() <= 230, "Chunk {} exceeds limit", i + 1);
    }
    
    // With typical lengths, might fit in 2 chunks
    assert!(chunks.len() <= 3, "Should not need more than 3 chunks for typical case, got {}", chunks.len());
}

// Helper function that mirrors the server's chunking logic
fn create_help_chunks(bbs_name: &str, friendly: &str, commands: &[&str]) -> Vec<String> {
    let max_chunk_size = 220; // Leave some margin below 230 bytes
    let mut chunks = Vec::new();
    
    // First chunk header
    let first_header = format!("[{}] Public Commands (for {}): ", bbs_name, friendly);
    let continuation_header = format!("[{}] More: ", bbs_name);
    
    let mut current_chunk = first_header.clone();
    let mut is_first_chunk = true;
    let mut commands_added = 0;
    
    for command in commands {
        let separator = if commands_added == 0 { "" } else { " | " };
        let test_chunk = format!("{}{}{}", current_chunk, separator, command);
        
        // Check if adding this command would exceed the limit
        if test_chunk.len() > max_chunk_size {
            // Finalize current chunk
            if is_first_chunk {
                current_chunk.push_str(" | DM for BBS access");
            }
            chunks.push(current_chunk);
            
            // Start new chunk
            current_chunk = format!("{}{}", continuation_header, command);
            is_first_chunk = false;
            commands_added = 1;
        } else {
            current_chunk = test_chunk;
            commands_added += 1;
        }
    }
    
    // Finalize last chunk
    if !current_chunk.is_empty() {
        if is_first_chunk {
            current_chunk.push_str(" | DM for BBS access");
        }
        chunks.push(current_chunk);
    }
    
    chunks
}