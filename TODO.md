# MeshBBS TODO

**Last Updated**: 2025-10-17  
**Branch**: `data_driven`  
**Status**: Alpha-ready with comprehensive data-driven systems

**Reference**: See `DATA_DRIVEN_STATUS.md` for complete system documentation

## Development Standards

**⚠️ CRITICAL: Zero Tolerance for Compiler Warnings**
- All warnings emitted by the Rust compiler must be fixed before committing
- All warnings in unit tests must be resolved
- Use `cargo check` and `cargo test` to verify clean builds
- This policy applies to all phases and contributions

**🚀 PERFORMANCE: Scale Target 500-1000 Users**
- ✅ All blocking database operations now async via spawn_blocking
- ✅ 27 async wrapper methods integrated into command processor
- ✅ Expected 5-10x performance improvement achieved
- All database queries must be O(1) or O(log n) where possible
- Use secondary indexes for frequently accessed data
- Pagination required for list operations over 100 items
- Monitor and test at target scale during development

## Legend
- [ ] TODO – not started
- [~] In progress – actively being worked on
- [x] Done – completed and tested
- [!] Blocked – waiting on dependency or decision

---

## Future Enhancements (Post-Alpha Launch)

**All core features are complete!** The items below are optional enhancements for future releases based on user feedback and requirements.

### Economy Enhancements
- [ ] Item quality/condition system for value degradation
- [ ] Reputation discounts based on player standing
- [ ] Bank vault storage for items (limited slots)
- [ ] Interest/fees configuration (optional, world-level)
- [ ] Bank NPC integration at specific locations
- [ ] Dynamic market prices based on supply/demand
- [ ] Auction house system
- [ ] Crafting system integration

### Social Features
- [ ] Player guilds/clans
- [ ] Guild chat channels
- [ ] Guild housing and shared resources
- [ ] Player reputation system
- [ ] Player-run shops and businesses
- [ ] In-game events and festivals

### Content Expansion
- [ ] Combat system (PvE and PvP)
- [ ] Magic system with spell casting
- [ ] Skills and leveling system
- [ ] Dungeon instances
- [ ] Boss encounters
- [ ] World events and dynamic content

### Technical Enhancements
- [~] Web-based admin dashboard (Phase 2 in progress on branch `tinymush_admin_webui`)
  - [x] Phase 1: Foundation (auth, audit, TLS, basic UI)
  - [x] Phase 2a: BBS user management API (list, view, update level)
  - [x] Phase 2b: Topics/messages API (list topics, list messages, topic stats)
  - [x] Phase 2c: Statistics API (comprehensive system metrics)
  - [~] Phase 2d: User management UI (in progress)
  - [ ] Phase 2e: Dashboard stats UI
  - [ ] Phase 3a: TinyMUSH seed management API (list/create/update/delete for all seed files)
  - [ ] Phase 3b: TinyMUSH runtime state controls (save, reload, wipe, backup download/upload)
  - [ ] Phase 3c: Admin UI list/detail editors for TinyMUSH seeds (rooms, NPCs, quests, etc.)
  - [ ] Phase 3d: Bulk operations and diff previews (multi-edit, import/export JSON)
  - [ ] Phase 3e: Audit logging + permissions for TinyMUSH actions
- [ ] Metrics and analytics system
- [ ] Localization support (multiple languages)

### Builder Commands Enhancement (Future)
- [ ] Builder undo/redo system
- [ ] Builder audit log for all creation/modification

---

## Quick Commands Reference

```bash
# Development cycle
cargo check                    # Fast syntax check
cargo test                     # Run all tests
cargo test --test <name>       # Run specific test file
cargo clippy                   # Lint check
cargo build --release          # Production build

# Git workflow
git add <files>
git commit -m "feat(system): description"
git push origin data_driven

# Documentation
mdbook serve docs              # View documentation locally
```

---

## Notes

### Current Status
- **387 tests passing** (all green)
- **Zero compiler warnings**
- **Alpha-ready** for deployment
- **All core systems complete** - see DATA_DRIVEN_STATUS.md

### Recent Completion
- Data-driven content systems fully implemented (Phases 1-6)
- All 6 admin commands working: @ACHIEVEMENT, @NPC, @COMPANION, @ROOM, @OBJECT, @QUEST
- JSON seed files in place for all content types
- Comprehensive documentation created
- Vending machine system with item cloning (Museum trinket vendor)
- Vendor NPC dialog integration complete (Mira the Vendor with 13 dialogue nodes)
- Shop system with clone-based and transfer-based purchasing

### Next Steps
Choose one of:
1. **Merge to main** - All systems tested and ready
2. **Extended testing** - Scale tests with 50-100 users
3. **Future enhancement** - Pick from list above
4. **Alpha deployment** - Begin user testing

---

**Project Status**: ✅ Alpha Complete  
**Test Coverage**: 387 tests passing  
**Documentation**: Complete  
**Next Action**: Merge `data_driven` → `main` or begin alpha testing
