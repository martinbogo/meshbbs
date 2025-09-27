# Fortune Module Development Guide

## Overview

The Fortune module (`src/bbs/fortune.rs`) implements the `^FORTUNE` public channel command, providing users with random wisdom quotes, programming humor, and inspirational messages.

## Architecture

### Core Components

- **Fortune Database**: Static array of 140 curated fortune entries
- **Random Selection**: Thread-safe random fortune picker using `rand::thread_rng()`
- **Helper Functions**: Utility functions for testing and diagnostics

### Integration Points

- **Public Command Parser** (`src/bbs/public.rs`): Recognizes `^FORTUNE` commands
- **Rate Limiting**: 5-second per-node cooldown via `PublicState::allow_fortune()`
- **Broadcast System**: Messages sent via public channel broadcast only

## Fortune Database

### Content Categories

1. **Classic Wisdom** (~15 entries): Socrates, Aristotle, Einstein, etc.
2. **Programming & Tech** (~25 entries): Industry wisdom and developer humor
3. **Literature & Philosophy** (~20 entries): Famous quotes and philosophical insights
4. **Motivational** (~30 entries): Inspirational and success-oriented messages
5. **Clean Humor** (~20 entries): Family-friendly jokes and wordplay
6. **Science & Discovery** (~15 entries): Scientific and research-oriented quotes
7. **Unix/Computing Culture** (~15 entries): Technical and Unix-philosophy quotes

### Quality Standards

- Maximum 200 characters per entry (mesh network optimized)
- No control characters (except ASCII whitespace)
- Public domain or widely-attributed content only
- Family-friendly and inclusive language

## API Reference

### Public Functions

```rust
/// Returns a random fortune from the database
pub fn get_fortune() -> &'static str

/// Returns the total number of fortunes (140)
pub fn fortune_count() -> usize

/// Returns the length of the longest fortune
pub fn max_fortune_length() -> usize
```

### Usage Example

```rust
use meshbbs::bbs::fortune::get_fortune;

let wisdom = get_fortune();
println!("Today's fortune: {}", wisdom);
```

## Testing Strategy

### Unit Tests (11 total)

1. **Database Validation**:
   - `fortunes_count_140`: Verifies exact count
   - `all_fortunes_under_200_chars`: Length validation
   - `all_fortunes_non_empty`: No empty strings
   - `all_fortunes_contain_printable_chars`: Character validation

2. **Functionality Tests**:
   - `fortune_returns_valid_response`: Basic function test
   - `fortune_randomness_check`: Distribution verification
   - `fortune_thread_safety_simulation`: Concurrent access test

3. **Quality Assurance**:
   - `fortune_database_quality_checks`: Content diversity verification
   - `fortune_count_matches_array`: Helper function consistency
   - `max_fortune_length_validation`: Length function validation
   - `helper_functions_consistency`: Cross-validation of utilities

### Integration Tests

- **fortune_behavior.rs**: Server integration and rate limiting tests
- **public_fortune.rs**: Command parsing and basic functionality

### Performance Characteristics

- **Memory Usage**: ~28KB static data (140 × ~200 bytes average)
- **CPU Usage**: O(1) random selection, minimal overhead
- **Thread Safety**: Full concurrent access support
- **Rate Limiting**: 5-second cooldown per node prevents spam

## Maintenance

### Adding New Fortunes

1. Add entries to `FORTUNES` array in `src/bbs/fortune.rs`
2. Update array size declaration: `const FORTUNES: [&str; NEW_COUNT]`
3. Update test expectation: `fortunes_count_X()` test function name and assertion
4. Update documentation: module docs, changelog, user guide
5. Run full test suite: `cargo test bbs::fortune`

### Content Guidelines

- Keep under 200 characters for mesh compatibility
- Attribute quotes when known (`— Author Name`)
- Use Unicode em dash (—) for attribution separator
- Avoid controversial, political, or offensive content
- Test for printable characters and proper encoding
- Verify public domain status or fair use

### Performance Monitoring

The module includes built-in diagnostics:

```rust
use meshbbs::bbs::fortune::{fortune_count, max_fortune_length};

println!("Fortune database: {} entries", fortune_count());
println!("Maximum fortune length: {} chars", max_fortune_length());
```

## Future Enhancements

### Potential Features

1. **Categories**: Themed fortune subsets (`^FORTUNE PROGRAMMING`, `^FORTUNE WISDOM`)
2. **External Database**: Load fortunes from external files
3. **User Favorites**: Allow users to save favorite fortunes
4. **Fortune of the Day**: Daily fortune rotation
5. **Localization**: Multi-language fortune support

### Implementation Considerations

- Maintain backward compatibility
- Preserve mesh network optimization (200 char limit)
- Consider storage implications for external databases
- Rate limiting adjustments for new features
- Documentation and test coverage for additions

## Troubleshooting

### Common Issues

1. **Compilation Errors**: Check array size matches content count
2. **Test Failures**: Verify character encoding and content guidelines
3. **Rate Limiting**: Ensure `PublicState` integration is correct
4. **Memory Usage**: Monitor static array size for embedded systems

### Debug Commands

```bash
# Run fortune-specific tests
cargo test bbs::fortune

# Run integration tests
cargo test fortune_behavior

# Check documentation examples
cargo test --doc

# Performance profiling
cargo bench --features fortune
```

## Related Documentation

- [Games Documentation](../docs/user-guide/games.md) - User-facing fortune documentation
- [Commands Reference](../docs/user-guide/commands.md) - Command syntax
- [Public Commands](src/bbs/public.rs) - Parser and rate limiting
- [Server Integration](src/bbs/server.rs) - Broadcast handling