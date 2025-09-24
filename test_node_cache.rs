#[cfg(test)]
mod tests {
    use meshbbs::meshtastic::NodeCache;
    use std::fs;

    #[test]
    fn test_node_cache() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing node cache functionality...");
    
    // Create a test cache
    let mut cache = NodeCache::new();
    
    // Add some test nodes
    cache.update_node(0x132BEE, "WAP2".to_string(), "WAP2".to_string());
    cache.update_node(0x0a132bee, "TestNode".to_string(), "TEST".to_string());
    
    println!("Created cache with {} nodes", cache.nodes.len());
    
    // Save to file
    let test_path = "test_node_cache.json";
    cache.save_to_file(test_path)?;
    println!("Saved cache to {}", test_path);
    
    // Load from file
    let loaded_cache = NodeCache::load_from_file(test_path)?;
    println!("Loaded cache with {} nodes", loaded_cache.nodes.len());
    
    // Verify data
    for (id, node) in &loaded_cache.nodes {
        println!("Node 0x{:08x}: {} ({})", id, node.long_name, node.short_name);
    }
    
    // Clean up
    fs::remove_file(test_path)?;
    println!("Test completed successfully!");
    
    Ok(())
    }
}