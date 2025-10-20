use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::tmush::types::{
    AchievementCategory, AchievementRecord, AchievementTrigger, CurrencyAmount, Direction,
    ObjectFlag, ObjectOwner, ObjectRecord, ObjectTrigger, RoomFlag, RoomRecord,
    OBJECT_SCHEMA_VERSION,
};

/// Required landing location where new characters are staged before entering the world.
pub const REQUIRED_LANDING_LOCATION_ID: &str = "gazebo_landing";

/// Required starting location where new characters enter after creation completes.
pub const REQUIRED_START_LOCATION_ID: &str = "town_square";

/// Prefix applied to per-player landing gazebo instances.
pub const LANDING_INSTANCE_PREFIX: &str = "gazebo_landing::";

/// Generate a unique landing gazebo instance identifier for a player.
pub fn generate_landing_instance_id(username: &str) -> String {
    let slug: String = username
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("{}{}:{}", LANDING_INSTANCE_PREFIX, slug, Uuid::new_v4())
}

/// Returns true if the provided room id is the canonical landing gazebo template.
pub fn is_landing_template(room_id: &str) -> bool {
    room_id == REQUIRED_LANDING_LOCATION_ID
}

/// Returns true if the provided room id represents a per-player landing gazebo instance.
pub fn is_personal_landing(room_id: &str) -> bool {
    room_id.starts_with(LANDING_INSTANCE_PREFIX)
}

/// Returns true if the room id corresponds to any landing gazebo (template or instance).
pub fn is_any_landing_room(room_id: &str) -> bool {
    is_landing_template(room_id) || is_personal_landing(room_id)
}

/// Old Towne Mesh sample world locations that ship as a reference implementation.
///
/// Only `REQUIRED_LANDING_LOCATION_ID` and `REQUIRED_START_LOCATION_ID` are
/// mandatory system requirements. The remaining locations are instructional
/// set dressing that operators are free to replace with their own layouts.
///
/// Total: 16 rooms (7 original + 8 expansion areas + 1 vertical space)
pub const OLD_TOWNE_WORLD_ROOM_IDS: &[&str] = &[
    // Core system rooms (required)
    REQUIRED_LANDING_LOCATION_ID,
    REQUIRED_START_LOCATION_ID,
    // Original locations (government, culture, commerce)
    "city_hall_lobby",
    "mayor_office",
    "mesh_museum",
    "north_gate",
    "south_market",
    // North expansion (wilderness, technology)
    "pine_ridge_trail",
    "repeater_tower",
    "repeater_upper", // Tower upper platform (accessed via UP)
    // West expansion (social, residential)
    "relay_tavern",
    "west_residential",
    // East expansion (nature, exploration)
    "forest_path",
    "ancient_grove",
    // South expansion (underground, crafting)
    "maintenance_tunnels",
    "workshop_district",
];

/// Build the canonical "Old Towne Mesh" starter rooms that Phase 1 seeds into the database.
///
/// The timestamps for each room are deterministic based on the `now` provided so tests can
/// supply a fixed value. Callers typically pass `Utc::now()` in production paths.
pub fn canonical_world_seed(now: DateTime<Utc>) -> Vec<RoomRecord> {
    let mut rooms = Vec::new();

    let landing = RoomRecord::world(
        REQUIRED_LANDING_LOCATION_ID,
        "Landing Gazebo",
        "A welcoming gazebo where new arrivals first materialize.",
        "You stand in an octagonal gazebo with polished wooden railings. Soft mesh lanterns \
cast a warm glow, and a carved wooden sign reads 'Welcome to Old Towne Mesh!' \
Through the northern archway, you can see the bustling Town Square. This is a \
safe place to learn the basics - try typing LOOK to examine your surroundings, \
INVENTORY to check what you're carrying, or HELP to see available commands. \
When ready, head NORTH to begin your adventure!",
    )
    .with_created_at(now)
    .with_exit(Direction::North, REQUIRED_START_LOCATION_ID)
    .with_flag(RoomFlag::Safe)
    .with_flag(RoomFlag::Indoor);
    rooms.push(landing);

    let town_square = RoomRecord::world(
        REQUIRED_START_LOCATION_ID,
        "Old Towne Square",
        "A tidy plaza centered around the Mesh beacon.",
        "Stone paths radiate from the beacon in the square's center. Mesh terminals hum
quietly while townsfolk trade stories about far-off packet relays.",
    )
    .with_created_at(now)
    .with_exit(Direction::North, "city_hall_lobby")
    .with_exit(Direction::East, "mesh_museum")
    .with_exit(Direction::West, "west_residential") // Link to residential
    .with_exit(Direction::South, "south_market")
    .with_exit(Direction::Down, "maintenance_tunnels") // Access to underground
    .with_capacity(25)
    .with_flag(RoomFlag::Safe)
    .with_flag(RoomFlag::Indoor);
    rooms.push(town_square);

    let city_hall = RoomRecord::world(
        "city_hall_lobby",
        "City Hall Lobby",
        "Sunlight filters through tall windows onto polished floors.",
        "Clerks shuffle reports about network outages while a patient queue waits to
register new callsigns. A portrait of the mayor watches over the proceedings.",
    )
    .with_created_at(now)
    .with_exit(Direction::South, REQUIRED_START_LOCATION_ID)
    .with_exit(Direction::North, "mayor_office")
    .with_exit(Direction::East, "mesh_museum")
    .with_flag(RoomFlag::Indoor)
    .with_flag(RoomFlag::Moderated);
    rooms.push(city_hall);

    let mayor_office = RoomRecord::world(
        "mayor_office",
        "Mayor's Office",
        "A well-appointed office with oak desk and mesh maps on walls.",
        "Mayor Thompson sits behind a sturdy oak desk, reviewing network topology
maps. Framed certificates line the walls alongside charts tracking mesh
uptime metrics. A window overlooks the town square.",
    )
    .with_created_at(now)
    .with_exit(Direction::South, "city_hall_lobby")
    .with_flag(RoomFlag::Safe)
    .with_flag(RoomFlag::Indoor)
    .with_flag(RoomFlag::QuestLocation);
    rooms.push(mayor_office);

    let museum = RoomRecord::world(
        "mesh_museum",
        "Mesh Museum",
        "Glass cases showcase legendary mesh hardware.",
        "Plaques describe pioneering nodes that kept the network alive during winter storms.
Interactive exhibits let visitors replay famous packet traces.",
    )
    .with_created_at(now)
    .with_exit(Direction::West, REQUIRED_START_LOCATION_ID)
    .with_exit(Direction::West, "city_hall_lobby") // Keep city hall connection
    .with_exit(Direction::North, "forest_path") // Link to nature areas
    .with_exit(Direction::South, "north_gate") // Keep existing
    .with_flag(RoomFlag::Indoor)
    .with_flag(RoomFlag::QuestLocation);
    rooms.push(museum);

    let north_gate = RoomRecord::world(
        "north_gate",
        "North Gate",
        "A sturdy archway opens toward pine forests and repeater towers.",
        "Guards wave as traders arrive with crates of modules bound for the workshop. Beyond the
path a narrow trail promises adventure past the ridge.",
    )
    .with_created_at(now)
    .with_exit(Direction::North, "pine_ridge_trail") // Link to wilderness
    .with_exit(Direction::West, "relay_tavern") // Link to tavern
    .with_exit(Direction::South, REQUIRED_START_LOCATION_ID)
    .with_exit(Direction::East, "mesh_museum")
    .with_flag(RoomFlag::QuestLocation);
    rooms.push(north_gate);

    let south_market = RoomRecord::world(
        "south_market",
        "South Market",
        "Stalls overflow with gadgets, produce, and hand-built antennas.",
        "Vendors haggle over packet quotas while musicians keep the tempo upbeat. The scent of
fresh bread mingles with solder flux drifting from a repair booth.",
    )
    .with_created_at(now)
    .with_exit(Direction::North, REQUIRED_START_LOCATION_ID)
    .with_exit(Direction::South, "workshop_district") // Link to crafting area
    .with_flag(RoomFlag::Shop)
    .with_flag(RoomFlag::Indoor)
    .with_capacity(18);
    rooms.push(south_market);

    // ========================================================================
    // EXPANSION AREAS - Educational examples for world builders
    // ========================================================================
    //
    // These rooms demonstrate various world design patterns:
    // - Transition areas that connect zones (pine_ridge_trail)
    // - Quest hubs with puzzles (repeater_tower)
    // - Social gathering spots (relay_tavern)
    // - Atmospheric/ambient locations (west_residential)
    // - Nature areas for companions (forest_path, ancient_grove)
    // - Dark exploration zones (maintenance_tunnels)
    // - Crafting/commerce extensions (workshop_district)

    // NORTH EXPANSION: Wilderness & Technology
    // Design pattern: Gateway → Trail → Destination
    // This creates a sense of journey and discovery

    // Connect North Gate to the wilderness areas
    // World builders: Use transition rooms to separate themed zones
    let pine_ridge = RoomRecord::world(
        "pine_ridge_trail",
        "Pine Ridge Trail",
        "A winding forest trail lined with towering pines.",
        "The packed earth trail winds upward through ancient pines. Sunlight filters through \
the canopy, dappling the ground with moving shadows. To the north, you can see a tall \
repeater tower rising above the treeline. The sounds of town fade behind you to the south.",
    )
    .with_created_at(now)
    .with_exit(Direction::South, "north_gate")
    .with_exit(Direction::North, "repeater_tower")
    .with_exit(Direction::East, "forest_path")
    .with_flag(RoomFlag::Safe) // Transition areas often safe
    .with_capacity(10);
    rooms.push(pine_ridge);

    // Quest hub with technical theme and puzzle potential
    // World builders: QuestLocation flag marks important areas
    let repeater_tower = RoomRecord::world(
        "repeater_tower",
        "Repeater Tower",
        "A tall communications tower bristling with antennas and cables.",
        "The mesh repeater tower stands three stories tall, its steel frame humming with radio \
activity. Solar panels angle toward the sun while wind turbines spin lazily overhead. A \
ladder leads UP to the maintenance platform above. Diagnostic panels blink with network status \
indicators. This relay node connects Old Towne to distant mesh communities.",
    )
    .with_created_at(now)
    .with_exit(Direction::South, "pine_ridge_trail")
    .with_exit(Direction::Up, "repeater_upper") // Climb to upper platform
    .with_flag(RoomFlag::QuestLocation)
    .with_capacity(5);
    rooms.push(repeater_tower);

    // Repeater Tower Upper Platform (accessed via UP from ground level)
    // World builders: Vertical navigation creates spatial depth
    let repeater_upper = RoomRecord::world(
        "repeater_upper",
        "Repeater Tower - Upper Platform",
        "The maintenance platform high above the tower base.",
        "You stand on a metal grating platform twenty feet above the ground. The wind is stronger \
up here, and you can see for miles across Old Towne and beyond. Three massive directional antennas \
dominate the platform - the NORTHERN ARRAY points toward Pine Ridge, the eastern array faces the \
forest, and the western one covers the residential areas. Control boxes and cable junctions are \
mounted on the central mast. A ladder leads DOWN to the ground level. The view is breathtaking \
but the height is dizzying.",
    )
    .with_created_at(now)
    .with_exit(Direction::Down, "repeater_tower") // Descend to ground level
    .with_flag(RoomFlag::QuestLocation)
    .with_flag(RoomFlag::Indoor) // Platform has roof/shelter
    .with_capacity(3); // Small platform, limited space
    rooms.push(repeater_upper);

    // WEST EXPANSION: Social & Residential
    // Design pattern: Create character and atmosphere through description

    // Social hub - tavern/pub gathering place
    // World builders: Social spaces benefit from Safe flag and higher capacity
    let tavern = RoomRecord::world(
        "relay_tavern",
        "The Relay Tavern",
        "A cozy tavern where mesh operators swap stories over drinks.",
        "Warm firelight flickers across wooden tables where locals gather to share news from \
distant nodes. The bar is carved from a single massive log, its surface worn smooth by \
countless elbows. Network topology maps cover one wall, marked with routes and relay \
points. The air smells of fresh bread and pine resin. This is where you hear the latest \
gossip and rumors.",
    )
    .with_created_at(now)
    .with_exit(Direction::East, "north_gate")
    .with_exit(Direction::South, "west_residential")
    .with_flag(RoomFlag::Safe)
    .with_flag(RoomFlag::Indoor)
    .with_capacity(20); // Social hubs need room for groups
    rooms.push(tavern);

    // Residential area with ambient NPCs
    // World builders: These areas provide context and immersion
    let west_lane = RoomRecord::world(
        "west_residential",
        "West Residential Lane",
        "A quiet lane lined with modest homes and small gardens.",
        "Comfortable homes line this peaceful street, their gardens bursting with vegetables \
and wildflowers. Solar panels on every roof feed power back to the mesh. Children's \
laughter echoes from a nearby yard. Mesh antennas sprout from rooftops like mechanical \
flowers. Residents nod friendly greetings as they tend their gardens or repair equipment.",
    )
    .with_created_at(now)
    .with_exit(Direction::North, "relay_tavern")
    .with_exit(Direction::East, REQUIRED_START_LOCATION_ID)
    .with_flag(RoomFlag::Safe)
    .with_capacity(15);
    rooms.push(west_lane);

    // EAST EXPANSION: Nature & Exploration
    // Design pattern: Natural areas for companion spawning and peaceful exploration

    // Nature trail for companion encounters
    // World builders: Outdoor areas good for companion/creature spawns
    let forest_path = RoomRecord::world(
        "forest_path",
        "Forest Path",
        "A narrow path meanders through dense woods.",
        "The forest path twists between massive oaks and maples. Birdsong fills the air, \
and deer tracks mark the soft earth. Sunlight breaks through the canopy in golden shafts. \
This peaceful place seems untouched by technology - nature holds dominion here. You might \
encounter wildlife: a loyal hound, a gentle horse, or perhaps a shadow cat.",
    )
    .with_created_at(now)
    .with_exit(Direction::West, "pine_ridge_trail")
    .with_exit(Direction::East, "ancient_grove")
    .with_exit(Direction::South, "mesh_museum")
    .with_flag(RoomFlag::Safe)
    .with_capacity(8);
    rooms.push(forest_path);

    // Destination location with quest potential
    // World builders: End-of-path locations work well for quest objectives
    let ancient_grove = RoomRecord::world(
        "ancient_grove",
        "Ancient Grove",
        "A mystical grove of towering ancient trees.",
        "Enormous trees form a natural cathedral here, their trunks wider than houses. Moss \
carpets the ground, and the air shimmers with an almost magical quality. This grove has \
stood for centuries, predating the mesh network by generations. Strange symbols are carved \
into some tree trunks - remnants of old rituals? The peaceful energy here seems to \
strengthen and restore those who visit.",
    )
    .with_created_at(now)
    .with_exit(Direction::West, "forest_path")
    .with_flag(RoomFlag::Safe)
    .with_flag(RoomFlag::QuestLocation)
    .with_capacity(6);
    rooms.push(ancient_grove);

    // SOUTH EXPANSION: Underground & Crafting
    // Design pattern: Dark/dangerous areas contrast with safe zones above

    // Dark exploration area - requires light source
    // World builders: Dark flag creates atmospheric exploration challenges
    let tunnels = RoomRecord::world(
        "maintenance_tunnels",
        "Maintenance Tunnels",
        "Dimly lit tunnels beneath the town streets.",
        "The underground maintenance tunnels carry mesh cables and power conduits beneath \
Old Towne. Emergency lights provide minimal illumination, casting eerie shadows. Water \
drips somewhere in the darkness. These tunnels connect to various parts of town - useful \
for quick travel if you don't mind the claustrophobic atmosphere and occasional strange \
echoes. Something scurries in the shadows...",
    )
    .with_created_at(now)
    .with_exit(Direction::North, REQUIRED_START_LOCATION_ID)
    .with_exit(Direction::Down, "maintenance_tunnels") // Loop for maze-like feel
    .with_exit(Direction::East, "workshop_district")
    .with_flag(RoomFlag::Dark) // Requires light source
    .with_flag(RoomFlag::Indoor)
    .with_capacity(5);
    rooms.push(tunnels);

    // Crafting and commerce hub
    // World builders: Shop flag enables vendor NPCs and trading
    let workshop = RoomRecord::world(
        "workshop_district",
        "Workshop District",
        "A maze of workshops and tool sheds filled with industrious activity.",
        "The workshop district buzzes with activity. Craftspeople bend over workbenches, \
soldering components and assembling modules. The air smells of hot metal and machine oil. \
Shelves overflow with parts: resistors, capacitors, antennas, cables. A bulletin board \
advertises custom builds and repair services. This is where mesh hardware is born, \
modified, and resurrected.",
    )
    .with_created_at(now)
    .with_exit(Direction::West, "maintenance_tunnels")
    .with_exit(Direction::North, "south_market")
    .with_flag(RoomFlag::Shop) // Commerce area
    .with_flag(RoomFlag::Indoor)
    .with_capacity(12);
    rooms.push(workshop);

    // ==================== PHASE 4 QUEST ROOMS ====================

    // CIPHER QUEST LOCATION (Phase 4.2)
    let cipher_chamber = RoomRecord::world(
        "cipher_chamber",
        "Cipher Chamber",
        "An ancient stone chamber with four seasonal glyphs.",
        "This circular chamber appears to be a place of learning from ages past. Four stone tablets \
stand at the cardinal directions, each carved with distinct seasonal symbols. The floor shows a faded \
mosaic of interconnected circles - perhaps an ancient visualization of communication networks. The air \
here feels heavy with knowledge, as if the stones themselves remember important secrets.",
    )
    .with_created_at(now)
    .with_exit(Direction::South, "ancient_grove") // Accessible from ancient grove
    .with_flag(RoomFlag::QuestLocation)
    .with_flag(RoomFlag::Safe)
    .with_flag(RoomFlag::Indoor)
    .with_capacity(4);
    rooms.push(cipher_chamber);

    // DARK NAVIGATION QUEST LOCATIONS (Phase 4.3)

    let deep_caverns = RoomRecord::world(
        "deep_caverns_entrance",
        "Deep Caverns Entrance",
        "The entrance to pitch-black caverns beneath Old Towne.",
        "The tunnel mouth yawns before you, descending into absolute darkness. Without a light source, \
you can see nothing beyond the first few feet. Cold air flows up from below, carrying the scent of \
damp stone and ancient earth. Rough-hewn steps lead downward into the unknown. This place has been \
sealed for decades - few know it even exists.",
    )
    .with_created_at(now)
    .with_exit(Direction::Up, "maintenance_tunnels") // Access from maintenance tunnels
    .with_exit(Direction::Down, "sunken_chamber")
    .with_flag(RoomFlag::Dark) // Requires light source
    .with_flag(RoomFlag::QuestLocation)
    .with_flag(RoomFlag::Indoor)
    .with_capacity(3);
    rooms.push(deep_caverns);

    let sunken_chamber = RoomRecord::world(
        "sunken_chamber",
        "Sunken Chamber",
        "A flooded chamber deep underground, utterly dark without light.",
        "Your light reflects off standing water covering the floor. The chamber is partially flooded, \
with water reaching mid-calf. Stalactites hang from the ceiling like ancient teeth. The walls show \
tool marks - this was carved by hand, long ago. A strange echo makes it hard to judge the chamber's \
true size. To the east, a narrow passage continues deeper.",
    )
    .with_created_at(now)
    .with_exit(Direction::Up, "deep_caverns_entrance")
    .with_exit(Direction::East, "hidden_vault")
    .with_flag(RoomFlag::Dark)
    .with_flag(RoomFlag::QuestLocation)
    .with_flag(RoomFlag::Indoor)
    .with_capacity(3);
    rooms.push(sunken_chamber);

    let hidden_vault = RoomRecord::world(
        "hidden_vault",
        "Hidden Vault",
        "A secret vault hidden in the deepest darkness, containing ancient artifacts.",
        "Your light reveals a treasure trove of pre-mesh artifacts. Metal shelves line the walls, \
holding ancient communication equipment: vacuum tubes, crystal sets, relay switches, and things you \
can't even identify. Everything is preserved in the constant temperature and dry air. A workbench \
holds tools and schematics. Someone used this as a workshop, then sealed it away. Why?",
    )
    .with_created_at(now)
    .with_exit(Direction::West, "sunken_chamber")
    .with_flag(RoomFlag::Dark)
    .with_flag(RoomFlag::QuestLocation)
    .with_flag(RoomFlag::Safe) // Once you reach it, it's safe
    .with_flag(RoomFlag::Indoor)
    .with_capacity(2);
    rooms.push(hidden_vault);

    // EPIC QUEST LOCATIONS (Phase 4.2-4.4 Combined)

    let forgotten_ruins = RoomRecord::world(
        "forgotten_ruins_entrance",
        "Forgotten Ruins Entrance",
        "Ancient ruins hidden in the wilderness beyond Old Towne.",
        "Crumbling stone walls emerge from thick vegetation. This place predates the mesh by centuries, \
possibly millennia. The entrance is flanked by four stone pillars, each bearing a distinct glyph. \
Vines have overtaken much of the structure, but the stonework beneath is solid. A sense of anticipation \
hangs in the air - this place has been waiting to be rediscovered.",
    )
    .with_created_at(now)
    .with_exit(Direction::South, "forest_path") // Beyond the ancient grove
    .with_exit(Direction::East, "ruins_dark_passage")
    .with_flag(RoomFlag::QuestLocation)
    .with_capacity(4);
    rooms.push(forgotten_ruins);

    let ruins_passage = RoomRecord::world(
        "ruins_dark_passage",
        "Dark Passage",
        "A pitch-black passage through the ancient ruins.",
        "Without light, you'd be completely lost here. The passage twists through solid rock, showing \
precision engineering that surpasses modern techniques. The walls are smooth as glass in places, \
rough-hewn in others. Symbols are carved at intervals - way markers? Warnings? Instructions? \
The passage opens into a larger chamber ahead.",
    )
    .with_created_at(now)
    .with_exit(Direction::West, "forgotten_ruins_entrance")
    .with_exit(Direction::North, "artifact_chamber")
    .with_flag(RoomFlag::Dark)
    .with_flag(RoomFlag::QuestLocation)
    .with_flag(RoomFlag::Indoor)
    .with_capacity(3);
    rooms.push(ruins_passage);

    let artifact_chamber = RoomRecord::world(
        "artifact_chamber",
        "Artifact Chamber",
        "The inner sanctum containing the legendary lost artifact.",
        "The chamber's centerpiece is a raised platform holding a sophisticated device under a crystal \
dome. Even centuries later, indicator lights still glow faintly. This is the legendary communication \
artifact - a masterwork combining technology and artistry. Inscriptions cover the walls in multiple \
languages, all saying the same thing: 'To connect is to understand. To understand is to unite.'",
    )
    .with_created_at(now)
    .with_exit(Direction::South, "ruins_dark_passage")
    .with_flag(RoomFlag::QuestLocation)
    .with_flag(RoomFlag::Safe)
    .with_flag(RoomFlag::Indoor)
    .with_capacity(2);
    rooms.push(artifact_chamber);

    rooms
}

/// Sample starter quests for Phase 6 Week 2
///
/// These quests demonstrate the quest system functionality and provide
/// new players with guided activities to explore Old Towne Mesh.
pub fn seed_starter_quests() -> Vec<crate::tmush::types::QuestRecord> {
    use crate::tmush::types::{CurrencyAmount, ObjectiveType, QuestObjective, QuestRecord};

    let mut quests = Vec::new();

    // Quest 1: Welcome to Old Towne (beginner)
    let welcome_quest = QuestRecord::new(
        "welcome_towne",
        "Welcome to Old Towne",
        "Explore the basics of Old Towne Mesh and meet important NPCs.",
        "mayor_thompson",
        1,
    )
    .with_objective(QuestObjective::new(
        "Visit the Town Square",
        ObjectiveType::VisitLocation {
            room_id: "town_square".to_string(),
        },
        1,
    ))
    .with_objective(QuestObjective::new(
        "Visit the Mesh Museum",
        ObjectiveType::VisitLocation {
            room_id: "mesh_museum".to_string(),
        },
        1,
    ))
    .with_objective(QuestObjective::new(
        "Talk to Mayor Thompson",
        ObjectiveType::TalkToNpc {
            npc_id: "mayor_thompson".to_string(),
        },
        1,
    ))
    .with_reward_currency(CurrencyAmount::Decimal { minor_units: 1000 }) // $10 or 100cp
    .with_reward_experience(50)
    .with_reward_item("town_badge");
    quests.push(welcome_quest);

    // Quest 2: Explore the Markets (requires quest 1)
    let market_quest = QuestRecord::new(
        "market_exploration",
        "Market Exploration",
        "Visit the South Market and learn about trading.",
        "mayor_thompson",
        2,
    )
    .with_objective(QuestObjective::new(
        "Visit South Market",
        ObjectiveType::VisitLocation {
            room_id: "south_market".to_string(),
        },
        1,
    ))
    .with_objective(QuestObjective::new(
        "Return to Town Square",
        ObjectiveType::VisitLocation {
            room_id: "town_square".to_string(),
        },
        1,
    ))
    .with_prerequisite("welcome_towne")
    .with_reward_currency(CurrencyAmount::Decimal { minor_units: 1500 }) // $15 or 150cp
    .with_reward_experience(75);
    quests.push(market_quest);

    // Quest 3: Network Explorer (advanced, requires quest 2)
    let explorer_quest = QuestRecord::new(
        "network_explorer",
        "Network Explorer",
        "Chart the full extent of Old Towne Mesh.",
        "mayor_thompson",
        3,
    )
    .with_objective(QuestObjective::new(
        "Visit all 7 locations",
        ObjectiveType::VisitLocation {
            room_id: "north_gate".to_string(),
        },
        1,
    ))
    .with_prerequisite("market_exploration")
    .with_reward_currency(CurrencyAmount::Decimal { minor_units: 2500 }) // $25 or 250cp
    .with_reward_experience(150)
    .with_reward_item("explorer_compass");
    quests.push(explorer_quest);

    quests
}

/// Generate starter achievements for Old Towne Mesh (Phase 6 Week 3)
pub fn seed_starter_achievements() -> Vec<AchievementRecord> {
    use AchievementCategory::*;
    use AchievementTrigger::*;

    let mut achievements = Vec::new();

    // Combat achievements
    achievements.push(
        AchievementRecord::new(
            "first_blood",
            "First Blood",
            "Defeat your first enemy",
            Combat,
            KillCount { required: 1 },
        )
        .with_title("the Brave"),
    );

    achievements.push(AchievementRecord::new(
        "veteran",
        "Veteran",
        "Defeat 100 enemies",
        Combat,
        KillCount { required: 100 },
    ));

    achievements.push(
        AchievementRecord::new(
            "legendary",
            "Legendary Warrior",
            "Defeat 1000 enemies",
            Combat,
            KillCount { required: 1000 },
        )
        .with_title("the Legendary")
        .as_hidden(),
    );

    // Exploration achievements
    achievements.push(
        AchievementRecord::new(
            "wanderer",
            "Wanderer",
            "Visit 10 unique rooms",
            Exploration,
            RoomVisits { required: 10 },
        )
        .with_title("the Wanderer"),
    );

    achievements.push(AchievementRecord::new(
        "explorer",
        "Explorer",
        "Visit 50 unique rooms",
        Exploration,
        RoomVisits { required: 50 },
    ));

    achievements.push(
        AchievementRecord::new(
            "cartographer",
            "Cartographer",
            "Visit all rooms in Old Towne",
            Exploration,
            RoomVisits { required: 100 },
        )
        .with_title("the Cartographer"),
    );

    // Social achievements
    achievements.push(AchievementRecord::new(
        "friendly",
        "Friendly",
        "Make 5 friends",
        Social,
        FriendCount { required: 5 },
    ));

    achievements.push(
        AchievementRecord::new(
            "popular",
            "Popular",
            "Make 20 friends",
            Social,
            FriendCount { required: 20 },
        )
        .with_title("the Popular"),
    );

    achievements.push(AchievementRecord::new(
        "chatterbox",
        "Chatterbox",
        "Send 1000 messages",
        Social,
        MessagesSent { required: 1000 },
    ));

    // Economic achievements
    achievements.push(
        AchievementRecord::new(
            "merchant",
            "Merchant",
            "Complete 50 trades",
            Economic,
            TradeCount { required: 50 },
        )
        .with_title("the Merchant"),
    );

    achievements.push(AchievementRecord::new(
        "wealthy",
        "Wealthy",
        "Earn 100000 currency",
        Economic,
        CurrencyEarned { amount: 100000 },
    ));

    // Quest achievements
    achievements.push(
        AchievementRecord::new(
            "quest_beginner",
            "Quest Beginner",
            "Complete your first quest",
            Quest,
            QuestCompletion { required: 1 },
        )
        .with_title("the Questor"),
    );

    achievements.push(AchievementRecord::new(
        "quest_veteran",
        "Quest Veteran",
        "Complete 25 quests",
        Quest,
        QuestCompletion { required: 25 },
    ));

    // Special/hidden achievements
    achievements.push(
        AchievementRecord::new(
            "town_founder",
            "Town Founder",
            "Discover the founder's secret",
            Special,
            VisitLocation {
                room_id: "mayor_office".to_string(),
            },
        )
        .with_title("Town Founder")
        .as_hidden(),
    );

    achievements.push(
        AchievementRecord::new(
            "network_pioneer",
            "Network Pioneer",
            "Complete the Network Explorer quest",
            Special,
            CompleteQuest {
                quest_id: "network_explorer".to_string(),
            },
        )
        .with_title("Network Pioneer"),
    );

    achievements
}

/// Seed starter companions for Old Towne Mesh (Phase 6 Week 4)
pub fn seed_starter_companions() -> Vec<crate::tmush::types::CompanionRecord> {
    use crate::tmush::types::{CompanionRecord, CompanionType};

    let mut companions = Vec::new();

    // A gentle horse available at the stable
    companions.push(
        CompanionRecord::new(
            "gentle_mare",
            "Gentle Mare",
            CompanionType::Horse,
            "south_market",
        )
        .with_description("A gentle brown mare with kind eyes. She seems eager for a rider."),
    );

    // A loyal dog at the town square
    companions.push(
        CompanionRecord::new(
            "loyal_hound",
            "Loyal Hound",
            CompanionType::Dog,
            REQUIRED_START_LOCATION_ID,
        )
        .with_description("A friendly dog with alert eyes. He wags his tail hopefully."),
    );

    // A mysterious cat near the museum
    companions.push(
        CompanionRecord::new(
            "shadow_cat",
            "Shadow Cat",
            CompanionType::Cat,
            "mesh_museum",
        )
        .with_description("A sleek black cat with piercing green eyes. She watches you intently."),
    );

    companions
}

/// Seed default crafting recipes for the world
/// Converts hardcoded recipes from the old system to data-driven database entries
pub fn seed_default_recipes() -> Vec<crate::tmush::types::CraftingRecipe> {
    use crate::tmush::types::CraftingRecipe;

    let mut recipes = Vec::new();

    // Recipe 1: Signal Booster (original hardcoded recipe)
    let signal_booster = CraftingRecipe::new(
        "signal_booster",
        "Signal Booster",
        "signal_booster",
        "world",
    )
    .with_description("A powerful signal booster for extended mesh range.")
    .with_material("copper_wire", 2)
    .with_material("circuit_board", 1)
    .with_material("antenna_rod", 1)
    .with_station("crafting_bench");
    recipes.push(signal_booster);

    // Recipe 2: Basic Antenna (original hardcoded recipe)
    let basic_antenna =
        CraftingRecipe::new("basic_antenna", "Basic Antenna", "basic_antenna", "world")
            .with_description("A simple antenna for basic mesh connectivity.")
            .with_material("copper_wire", 2)
            .with_material("antenna_rod", 1)
            .with_station("crafting_bench");
    recipes.push(basic_antenna);

    recipes
}

/// Seed starter NPCs for Old Towne Mesh
pub fn seed_starter_npcs() -> Vec<crate::tmush::types::NpcRecord> {
    use crate::tmush::types::NpcRecord;

    let mut npcs = Vec::new();

    // Mayor Thompson - Tutorial completion NPC
    let mayor = NpcRecord::new(
        "mayor_thompson",
        "Mayor Thompson",
        "Mayor of Old Towne Mesh",
        "A distinguished figure in formal attire. Mayor Thompson greets visitors with \
a warm smile and firm handshake. Years of network administration show in the \
confident way he discusses mesh topology.",
        "mayor_office",
    )
    .with_dialog(
        "greeting",
        "Welcome to Old Towne Mesh! I'm Mayor Thompson. \
I oversee the network operations here. How can I help you today?",
    )
    .with_dialog(
        "tutorial_complete",
        "Excellent work completing the tutorial! You've learned the basics of navigating \
our world. Here's a small reward to get you started on your adventures. \
Welcome to Old Towne Mesh, citizen!",
    )
    .with_dialog(
        "quest_welcome",
        "Looking for something to do? I have some tasks that could use a capable \
adventurer like yourself. Check back when you're ready for a challenge!",
    );

    npcs.push(mayor);

    // City Hall Clerk - Administrative help
    let clerk = NpcRecord::new(
        "city_clerk",
        "City Clerk",
        "Administrative Clerk",
        "A busy clerk with wire-rimmed glasses shuffles through papers. She looks up \
with a professional smile, ready to assist with administrative matters.",
        "city_hall_lobby",
    )
    .with_dialog(
        "greeting",
        "Welcome to City Hall! I handle administrative matters. If you need help \
understanding how things work around here, just ask!",
    )
    .with_dialog(
        "help",
        "Old Towne Mesh operates on a mesh network. You can explore the town, \
complete quests, trade with others, and even claim housing! Type HELP for commands.",
    )
    .with_flag(crate::tmush::types::NpcFlag::TutorialNpc);

    npcs.push(clerk);

    // Gate Guard - Security and direction
    let guard = NpcRecord::new(
        "gate_guard",
        "Gate Guard",
        "North Gate Guard",
        "A weathered guard in practical gear stands watch. Years of patrol have given \
her a keen eye for travelers. She nods in acknowledgment as you approach.",
        "north_gate",
    )
    .with_dialog(
        "greeting",
        "Greetings, traveler. I keep watch over the northern approach. Beyond lies \
the wilderness - beautiful but dangerous for the unprepared.",
    )
    .with_dialog(
        "warning",
        "If you venture beyond the gate, be sure you're properly equipped. The mesh \
signal weakens past the ridge, and you'll be on your own out there.",
    )
    .with_flag(crate::tmush::types::NpcFlag::Guard);

    npcs.push(guard);

    // Market Vendor - Trading and commerce
    let vendor = NpcRecord::new(
        "market_vendor",
        "Mira the Vendor",
        "South Market Trader",
        "A cheerful vendor behind a stall piled with mesh hardware and local goods. \
Mira has a reputation for fair prices and interesting stories about the items she sells.",
        "south_market",
    )
    .with_dialog(
        "greeting",
        "Welcome to my stall! I've got the finest mesh components and supplies in \
Old Towne. Looking for anything particular?",
    )
    .with_dialog(
        "wares",
        "I stock everything from basic antennas to rare modules salvaged from old nodes. \
Right now I'm running low on inventory, but check back soon - I restock regularly!",
    )
    .with_dialog(
        "story",
        "This stall has been in my family for three generations. My grandmother was one \
of the original mesh pioneers. These goods carry that legacy!",
    )
    .with_flag(crate::tmush::types::NpcFlag::Vendor);

    npcs.push(vendor);

    // Museum Curator - Lore and history
    let curator = NpcRecord::new(
        "museum_curator",
        "Dr. Reeves",
        "Museum Curator",
        "An elderly scholar with kind eyes and countless stories. Dr. Reeves has \
dedicated decades to preserving the history of the mesh network.",
        "mesh_museum",
    )
    .with_dialog(
        "greeting",
        "Ah, welcome to the Mesh Museum! I'm Dr. Reeves, curator and historian. \
Every artifact here tells a story of our network's resilience.",
    )
    .with_dialog(
        "history",
        "This museum chronicles the early days of the mesh - the pioneers who kept \
packets flowing through blizzards and power outages. Each node had a story, \
each relay a hero behind it.",
    )
    .with_dialog(
        "exhibit",
        "That display there? That's the original relay from Winter Storm '19. \
It ran for 72 hours on backup power, keeping the southern district connected. \
Legendary piece of equipment.",
    )
    .with_flag(crate::tmush::types::NpcFlag::TutorialNpc);

    npcs.push(curator);

    npcs
}

/// Seed starter vending machine with trinkets
pub fn seed_starter_vending_machine() -> (
    crate::tmush::shop::ShopRecord,
    Vec<crate::tmush::types::ObjectRecord>,
) {
    use crate::tmush::shop::{ShopItem, ShopRecord};
    use crate::tmush::types::{CurrencyAmount, ObjectFlag, ObjectRecord};

    // Create vending machine shop
    let mut vending_machine = ShopRecord::new_vending_machine(
        "vm_museum".to_string(),
        "Museum Trinket Vending Machine".to_string(),
        "mesh_museum".to_string(),
    );

    vending_machine.description = "A sleek vending machine filled with commemorative trinkets and \
        museum souvenirs. Insert coins and press the button to receive your item."
        .to_string();

    // Create 4 trinket objects to sell
    let mut trinkets = Vec::new();

    // Trinket 1: Mesh Network Keychain - 25 coins
    let mut keychain = ObjectRecord::new_world(
        "trinket_keychain",
        "Mesh Network Keychain",
        "A small metal keychain shaped like a mesh network node. The logo glows faintly \
            in the dark. Perfect for attaching to your gear.",
    );
    keychain.currency_value = CurrencyAmount::multi_tier(25);
    keychain.weight = 1;
    keychain.flags = vec![ObjectFlag::Clonable];
    keychain.takeable = true;
    trinkets.push(keychain);
    vending_machine
        .add_item(ShopItem::limited("trinket_keychain".to_string(), 50))
        .ok();

    // Trinket 2: Commemorative Antenna Pin - 50 coins
    let mut pin = ObjectRecord::new_world(
        "trinket_pin",
        "Commemorative Antenna Pin",
        "A silver pin shaped like the town's famous relay antenna. Engraved with \
            'Old Towne Mesh - Est. 2015'. A fashionable way to show your community pride.",
    );
    pin.currency_value = CurrencyAmount::multi_tier(50);
    pin.weight = 1;
    pin.flags = vec![ObjectFlag::Clonable];
    pin.takeable = true;
    trinkets.push(pin);
    vending_machine
        .add_item(ShopItem::limited("trinket_pin".to_string(), 30))
        .ok();

    // Trinket 3: Storm '19 Memorial Coin - 75 coins
    let mut coin = ObjectRecord::new_world(
        "trinket_coin",
        "Storm '19 Memorial Coin",
        "A brass commemorative coin honoring the legendary 72-hour relay operation \
            during the Winter Storm of 2019. One side shows the relay tower, the other reads \
            'Connected Through the Storm'. A collector's item.",
    );
    coin.currency_value = CurrencyAmount::multi_tier(75);
    coin.weight = 2;
    coin.flags = vec![ObjectFlag::Clonable];
    coin.takeable = true;
    trinkets.push(coin);
    vending_machine
        .add_item(ShopItem::limited("trinket_coin".to_string(), 20))
        .ok();

    // Trinket 4: Museum Replica Node - 100 coins
    let mut replica = ObjectRecord::new_world(
        "trinket_replica",
        "Museum Replica Node",
        "A detailed miniature replica of the original mesh node displayed in the museum. \
            Complete with tiny LED lights that blink in a realistic pattern. This exquisite piece is \
            handcrafted by local artisans and makes an excellent desk ornament.",
    );
    replica.currency_value = CurrencyAmount::multi_tier(100);
    replica.weight = 5;
    replica.flags = vec![ObjectFlag::Clonable];
    replica.takeable = true;
    trinkets.push(replica);
    vending_machine
        .add_item(ShopItem::limited("trinket_replica".to_string(), 10))
        .ok();

    (vending_machine, trinkets)
}

/// Seed full dialogue trees for all NPCs (called after seed_npcs_if_needed)
pub fn seed_npc_dialogues_if_needed(
    store: &crate::tmush::storage::TinyMushStore,
) -> Result<usize, crate::tmush::errors::TinyMushError> {
    use crate::tmush::types::{DialogAction, DialogChoice, DialogCondition, DialogNode};
    use std::collections::HashMap;

    let mut updated = 0;

    // Mayor Thompson - Tutorial and town info
    match store.get_npc("mayor_thompson") {
        Ok(mut mayor) => {
            if mayor.dialog_tree.is_empty() {
                let mut tree = HashMap::new();

                // Root greeting node
                tree.insert("greeting".to_string(), DialogNode::new(
                "Welcome to Old Towne Mesh! I'm Mayor Thompson, and I oversee operations here. \
                Whether you're just starting out or looking for what to do next, I'm here to help!"
            )
            .with_choice(DialogChoice::new("Tell me about the tutorial").goto("tutorial"))
            .with_choice(DialogChoice::new("What is there to do in town?").goto("town_info"))
            .with_choice(DialogChoice::new("Do you have any quests?").goto("quest_check"))
            .with_choice(DialogChoice::new("Thanks, I'll look around").exit()));

                // Tutorial branch
                tree.insert("tutorial".to_string(), DialogNode::new(
                "The tutorial will teach you the basics of navigating our world - movement, talking to NPCs, \
                using items, and more. It's a great way to get your bearings!"
            )
            .with_choice(DialogChoice::new("How do I start the tutorial?").goto("tutorial_start"))
            .with_choice(DialogChoice::new("What commands should I know?").goto("tutorial_commands"))
            .with_choice(DialogChoice::new("I'd rather skip it").goto("skip_tutorial"))
            .with_choice(DialogChoice::new("Let me think about it").exit()));

                tree.insert("tutorial_start".to_string(), DialogNode::new(
                "Just type START TUTORIAL and you'll begin! The tutorial will guide you through each step. \
                When you complete it, come back and I'll have a small reward for you."
            )
            .with_choice(DialogChoice::new("Thanks, I'll try it!").exit())
            .with_choice(DialogChoice::new("What else is there?").goto("town_info")));

                tree.insert("tutorial_commands".to_string(), DialogNode::new(
                "Basic commands: LOOK (examine surroundings), GO <direction> (move), TALK <npc> (chat), \
                INVENTORY (check items), HELP (full command list). The tutorial covers all of these!"
            )
            .with_choice(DialogChoice::new("Got it!").exit())
            .with_choice(DialogChoice::new("Tell me about town").goto("town_info")));

                tree.insert("skip_tutorial".to_string(), DialogNode::new(
                "That's fine! You can always start it later with START TUTORIAL. Feel free to explore \
                on your own - the City Clerk can help with questions."
            )
            .with_choice(DialogChoice::new("Thanks").exit()));

                // Town info branch
                tree.insert("town_info".to_string(), DialogNode::new(
                "Old Towne Mesh has everything you need! We have a museum of mesh history, a marketplace, \
                City Hall for administrative help, and gates leading to the wilderness."
            )
            .with_choice(DialogChoice::new("Tell me about housing").goto("housing_info"))
            .with_choice(DialogChoice::new("What about quests?").goto("quest_check"))
            .with_choice(DialogChoice::new("Thanks!").exit()));

                tree.insert("housing_info".to_string(), DialogNode::new(
                "You can claim housing through City Hall! Talk to the City Clerk about available options. \
                Having your own place lets you store items and customize your space."
            )
            .with_choice(DialogChoice::new("Sounds great!").exit())
            .with_choice(DialogChoice::new("What else?").goto("town_info")));

                // Quest branch
                tree.insert("quest_check".to_string(), DialogNode::new(
                "I have some tasks that need doing, but I like to make sure folks are ready first. \
                Complete the tutorial and get familiar with town, then come back!"
            )
            .with_choice(DialogChoice::new("Will do!").exit())
            .with_choice(DialogChoice::new("Tell me about town").goto("town_info")));

                mayor.dialog_tree = tree;
                store.put_npc(mayor)?;
                updated += 1;
            }
        }
        Err(e) => {
            eprintln!(
                "Warning: Could not load mayor_thompson for dialogue seeding: {}",
                e
            );
        }
    }

    // City Clerk - Administrative help
    match store.get_npc("city_clerk") {
        Ok(mut clerk) => {
            if clerk.dialog_tree.is_empty() {
                let mut tree = HashMap::new();

                // Root greeting node
                tree.insert("greeting".to_string(), DialogNode::new(
                    "Welcome to City Hall! I'm here to help with administrative matters. Whether you need \
                    information about housing, quests, or town services, I can point you in the right direction."
                )
                .with_choice(DialogChoice::new("Tell me about housing").goto("housing_help"))
                .with_choice(DialogChoice::new("What quests are available?").goto("quest_help"))
                .with_choice(DialogChoice::new("What services does City Hall offer?").goto("services"))
                .with_choice(DialogChoice::new("Thanks, I'm all set").exit()));

                // Housing branch
                tree.insert("housing_help".to_string(), DialogNode::new(
                    "City Hall manages the housing system! You can claim an apartment or larger space \
                    depending on what you can afford. Each housing unit is yours to customize and store items."
                )
                .with_choice(DialogChoice::new("How much does housing cost?").goto("housing_cost"))
                .with_choice(DialogChoice::new("What else can you tell me?").goto("services"))
                .with_choice(DialogChoice::new("I'll think about it").exit()));

                tree.insert("housing_cost".to_string(), DialogNode::new(
                    "We have several options! Studio apartments start at 100 credits to claim, with a small \
                    recurring fee. Larger apartments with multiple rooms cost more but give you more space. \
                    Check the housing board for current availability."
                )
                .with_choice(DialogChoice::new("Sounds good!").exit())
                .with_choice(DialogChoice::new("Tell me about other services").goto("services")));

                // Quest branch
                tree.insert("quest_help".to_string(), DialogNode::new(
                    "Various people around town have tasks they need help with. Mayor Thompson coordinates \
                    most quest activities. You can also check the bulletin boards for opportunities!"
                )
                .with_choice(DialogChoice::new("Where should I look?").goto("quest_locations"))
                .with_choice(DialogChoice::new("What else can you help with?").goto("services"))
                .with_choice(DialogChoice::new("Thanks!").exit()));

                tree.insert("quest_locations".to_string(), DialogNode::new(
                    "The Mayor's office is a great place to start. The museum curator, market vendors, and \
                    even the gate guards sometimes have work available. Keep your eyes open!"
                )
                .with_choice(DialogChoice::new("Got it!").exit())
                .with_choice(DialogChoice::new("Tell me about housing").goto("housing_help")));

                // Services branch
                tree.insert("services".to_string(), DialogNode::new(
                    "City Hall handles housing claims, maintains public records, and coordinates with \
                    various town departments. We also keep bulletin boards updated with community news and \
                    opportunities. If you ever have questions about how things work, come find me!"
                )
                .with_choice(DialogChoice::new("Very helpful, thanks!").exit())
                .with_choice(DialogChoice::new("About housing...").goto("housing_help"))
                .with_choice(DialogChoice::new("About quests...").goto("quest_help")));

                clerk.dialog_tree = tree;
                store.put_npc(clerk)?;
                updated += 1;
            }
        }
        Err(e) => {
            eprintln!(
                "Warning: Could not load city_clerk for dialogue seeding: {}",
                e
            );
        }
    }

    // Gate Guard - Security and warnings
    match store.get_npc("gate_guard") {
        Ok(mut guard) => {
            if guard.dialog_tree.is_empty() {
                let mut tree = HashMap::new();

                // Root greeting node
                tree.insert("greeting".to_string(), DialogNode::new(
                    "Greetings, traveler. I keep watch over the northern approach. Beyond this gate lies \
                    the wilderness - beautiful but dangerous for those unprepared."
                )
                .with_choice(DialogChoice::new("What's out there?").goto("looking"))
                .with_choice(DialogChoice::new("What dangers should I know about?").goto("dangers"))
                .with_choice(DialogChoice::new("Any advice for exploring?").goto("advice"))
                .with_choice(DialogChoice::new("Just passing through").exit()));

                // Looking branch
                tree.insert("looking".to_string(), DialogNode::new(
                    "The wilderness holds many opportunities - rare resources, ancient relics, and \
                    untamed beauty. But it's also home to hostile creatures and environmental hazards. \
                    The mesh signal weakens the farther you go."
                )
                .with_choice(DialogChoice::new("What kind of dangers?").goto("dangers"))
                .with_choice(DialogChoice::new("Should I go out there?").goto("adventure_warning"))
                .with_choice(DialogChoice::new("I'll be careful").exit()));

                // Dangers branch
                tree.insert("dangers".to_string(), DialogNode::new(
                    "Wild animals, unstable terrain, signal dead zones... The wilderness doesn't forgive \
                    mistakes. Many venture out unprepared and need rescue. Some never come back."
                )
                .with_choice(DialogChoice::new("How should I prepare?").goto("equipment"))
                .with_choice(DialogChoice::new("Maybe I should stay in town").goto("adventure_warning"))
                .with_choice(DialogChoice::new("Thanks for the warning").exit()));

                tree.insert("adventure_warning".to_string(), DialogNode::new(
                    "Smart thinking. Build your skills in town first. Complete some quests, gather supplies, \
                    and learn your way around. The wilderness will still be here when you're ready."
                )
                .with_choice(DialogChoice::new("Good advice").exit())
                .with_choice(DialogChoice::new("Where's the market?").goto("market_direction")));

                // Equipment branch
                tree.insert("equipment".to_string(), DialogNode::new(
                    "At minimum, bring food, water, and basic tools. A reliable signal booster helps maintain \
                    connectivity. The market district has supplies - Mira's stall is your best bet."
                )
                .with_choice(DialogChoice::new("Where's the market?").goto("market_direction"))
                .with_choice(DialogChoice::new("Thanks, I'll gear up").exit()));

                tree.insert("market_direction".to_string(), DialogNode::new(
                    "Head south from the town square. You can't miss it - lots of stalls and activity. \
                    Mira runs a good operation, fair prices and quality gear."
                )
                .with_choice(DialogChoice::new("Thanks!").exit()));

                // Advice branch
                tree.insert("advice".to_string(), DialogNode::new(
                    "Never go out alone on your first trip. Stay within sight of the walls until you know \
                    what you're doing. And always tell someone where you're going. The rescue teams can only \
                    help if they know where to look."
                )
                .with_choice(DialogChoice::new("I'll remember that").exit())
                .with_choice(DialogChoice::new("Tell me about the dangers").goto("dangers")));

                guard.dialog_tree = tree;
                store.put_npc(guard)?;
                updated += 1;
            }
        }
        Err(e) => {
            eprintln!(
                "Warning: Could not load gate_guard for dialogue seeding: {}",
                e
            );
        }
    }

    // Market Vendor - Trading and commerce
    match store.get_npc("market_vendor") {
        Ok(mut vendor) => {
            if vendor.dialog_tree.is_empty() {
                let mut tree = HashMap::new();

                // Root greeting node
                tree.insert("greeting".to_string(), DialogNode::new(
                    "Welcome to my stall! I'm Mira, and I've got the finest mesh components and supplies \
                    in Old Towne. Everything from basic tools to rare salvaged tech!"
                )
                .with_choice(DialogChoice::new("What do you have for sale?").goto("wares"))
                .with_choice(DialogChoice::new("Tell me about yourself").goto("story"))
                .with_choice(DialogChoice::new("I'd like to buy something").goto("buying"))
                .with_choice(DialogChoice::new("Just browsing, thanks").exit()));

                // Wares branch
                tree.insert("wares".to_string(), DialogNode::new(
                    "I stock everything a traveler needs! Basic tools, signal boosters, mesh hardware, \
                    and the occasional rare find. My inventory changes as I acquire new goods, so check back often!"
                )
                .with_choice(DialogChoice::new("What's your best item?").goto("best_item"))
                .with_choice(DialogChoice::new("Can I buy something now?").goto("buying"))
                .with_choice(DialogChoice::new("Tell me your story").goto("story"))
                .with_choice(DialogChoice::new("Thanks!").exit()));

                tree.insert("best_item".to_string(), DialogNode::new(
                    "Right now? I've got a mesh signal booster that's perfect for wilderness exploration. \
                    Keeps you connected even in weak signal areas. 50 credits and it's yours!"
                )
                .with_choice(DialogChoice::new("I'll take it!").goto("buy_booster"))
                .with_choice(DialogChoice::new("Maybe later").exit()));

                // Story branch
                tree.insert("story".to_string(), DialogNode::new(
                    "This stall has been in my family for three generations! My grandmother was one of the \
                    original mesh pioneers. She taught me that quality goods and fair dealing build lasting networks."
                )
                .with_choice(DialogChoice::new("That's a great tradition!").exit())
                .with_choice(DialogChoice::new("What do you sell?").goto("wares"))
                .with_choice(DialogChoice::new("Tell me more history").goto("history_detail")));

                tree.insert("history_detail".to_string(), DialogNode::new(
                    "My grandmother kept the network running during the great storms of '19. Every piece \
                    of hardware here carries that legacy - built to last, tested in harsh conditions."
                )
                .with_choice(DialogChoice::new("Impressive!").exit())
                .with_choice(DialogChoice::new("Show me what you have").goto("wares")));

                // Buying branch
                tree.insert("buying".to_string(), DialogNode::new(
                    "Wonderful! I have some items ready for immediate purchase, or you can browse my full \
                    inventory at the shop interface."
                )
                .with_choice(DialogChoice::new("What's available now?").goto("prices"))
                .with_choice(DialogChoice::new("Never mind").exit()));

                tree.insert("prices".to_string(), DialogNode::new(
                    "I've got a few items on hand:\n\n\
                    • Basic Knife - 15 credits. Handy for cutting rope and general tasks.\n\
                    • Signal Booster - 50 credits. Essential for wilderness travel, extends range by 50%.\n\n\
                    What would you like?"
                )
                .with_choice(DialogChoice::new("I'll buy the knife (15 credits)")
                    .goto("purchase_knife")
                    .with_condition(DialogCondition::HasCurrency { amount: 15 }))
                .with_choice(DialogChoice::new("I'll buy the booster (50 credits)")
                    .goto("buy_booster")
                    .with_condition(DialogCondition::HasCurrency { amount: 50 }))
                .with_choice(DialogChoice::new("I need to earn more credits first")
                    .goto("earning_tips"))
                .with_choice(DialogChoice::new("Just looking for now").exit()));

                // Earning tips for players who need more credits
                tree.insert("earning_tips".to_string(), DialogNode::new(
                    "Plenty of ways to earn credits! Complete quests from the Mayor or other NPCs, \
                    craft items at the workshop and sell them, explore the wilderness for valuable salvage, \
                    or help other players with their projects. The mesh economy rewards hard work!"
                )
                .with_choice(DialogChoice::new("Good to know!").exit())
                .with_choice(DialogChoice::new("Show me what you have again").goto("prices")));

                // Purchase nodes with actual transactions
                tree.insert("purchase_knife".to_string(), DialogNode::new(
                    "Excellent choice! Here's your knife - keep it sharp and it'll serve you well. \
                    That's 15 credits. Pleasure doing business with you!"
                )
                .with_action(DialogAction::TakeCurrency { amount: 15 })
                .with_action(DialogAction::GiveItem { 
                    item_id: "vendor_basic_knife".to_string(), 
                    quantity: 1 
                })
                .with_choice(DialogChoice::new("Thanks!").exit())
                .with_choice(DialogChoice::new("What else do you have?").goto("prices")));

                tree.insert("buy_booster".to_string(), DialogNode::new(
                    "Smart investment! This signal booster is top quality - built by my family's shop. \
                    It'll keep you connected even in the deep wilderness. That's 50 credits. \
                    Safe travels, friend!"
                )
                .with_action(DialogAction::TakeCurrency { amount: 50 })
                .with_action(DialogAction::GiveItem { 
                    item_id: "vendor_signal_booster".to_string(), 
                    quantity: 1 
                })
                .with_choice(DialogChoice::new("Thanks!").exit())
                .with_choice(DialogChoice::new("What else?").goto("prices")));

                // Additional info branches
                tree.insert("valuable_items".to_string(), DialogNode::new(
                    "Looking for something special? I occasionally get rare mesh components from salvagers. \
                    Old relay modules, vintage antennas, that sort of thing. Collectors pay top credit!"
                )
                .with_choice(DialogChoice::new("How do I find those?").goto("finding_valuables"))
                .with_choice(DialogChoice::new("Interesting!").exit()));

                tree.insert("finding_valuables".to_string(), DialogNode::new(
                    "Explore the wilderness, check abandoned sites, complete quests. Bring me anything \
                    interesting you find - I'll give you a fair price or trade you something useful!"
                )
                .with_choice(DialogChoice::new("Will do!").exit())
                .with_choice(DialogChoice::new("Show me your current stock").goto("wares")));

                tree.insert("weapons".to_string(), DialogNode::new(
                    "I keep a few basic weapons for self-defense. Nothing fancy, but reliable. The knife is \
                    practical for everyday carry, and I sometimes have staves or other tools."
                )
                .with_choice(DialogChoice::new("I'll take the knife").goto("purchase_knife"))
                .with_choice(DialogChoice::new("What else do you have?").goto("wares"))
                .with_choice(DialogChoice::new("Maybe later").exit()));

                vendor.dialog_tree = tree;
                store.put_npc(vendor)?;
                updated += 1;
            }
        }
        Err(e) => {
            eprintln!(
                "Warning: Could not load market_vendor for dialogue seeding: {}",
                e
            );
        }
    }

    // Museum Curator - Lore and history
    match store.get_npc("museum_curator") {
        Ok(mut curator) => {
            if curator.dialog_tree.is_empty() {
                let mut tree = HashMap::new();

                // Root greeting node
                tree.insert("greeting".to_string(), DialogNode::new(
                    "Ah, welcome to the Mesh Museum! I'm Dr. Reeves, curator and historian. Every artifact \
                    here tells a story of our network's resilience and the people who built it."
                )
                .with_choice(DialogChoice::new("Tell me about the museum").goto("about_museum"))
                .with_choice(DialogChoice::new("What's the history here?").goto("history"))
                .with_choice(DialogChoice::new("Any special exhibits?").goto("exhibit"))
                .with_choice(DialogChoice::new("Can I help with research?").goto("research_offer"))
                .with_choice(DialogChoice::new("Just looking around").exit()));

                // Museum branch
                tree.insert("about_museum".to_string(), DialogNode::new(
                    "This museum preserves the history of the mesh network - from the first nodes to today. \
                    We document the technology, the people, and the challenges they overcame to keep packets flowing."
                )
                .with_choice(DialogChoice::new("Who founded it?").goto("founder"))
                .with_choice(DialogChoice::new("Tell me about the history").goto("history"))
                .with_choice(DialogChoice::new("Fascinating!").exit()));

                tree.insert("founder".to_string(), DialogNode::new(
                    "The museum was founded by the original mesh pioneers - people like Mira's grandmother. \
                    They wanted future generations to understand what it took to build this network from nothing."
                )
                .with_choice(DialogChoice::new("Amazing dedication").exit())
                .with_choice(DialogChoice::new("Tell me more history").goto("history_detail")));

                // History branch
                tree.insert("history".to_string(), DialogNode::new(
                    "The mesh network started small - just a few nodes connecting neighbors. Through \
                    determination and ingenuity, it grew into the robust system we have today. Every relay \
                    has a story, every node a guardian."
                )
                .with_choice(DialogChoice::new("Tell me more").goto("history_detail"))
                .with_choice(DialogChoice::new("What about the exhibits?").goto("exhibit"))
                .with_choice(DialogChoice::new("Impressive").exit()));

                tree.insert("history_detail".to_string(), DialogNode::new(
                    "Take the Winter Storm of '19 - power was out for days across the region. But the mesh \
                    stayed up! Battery backups, solar panels, people sharing generators... The network became \
                    a lifeline for the community."
                )
                .with_choice(DialogChoice::new("That's incredible!").exit())
                .with_choice(DialogChoice::new("Tell me about that relay").goto("relay_story"))
                .with_choice(DialogChoice::new("What else is here?").goto("exhibit")));

                tree.insert("relay_story".to_string(), DialogNode::new(
                    "That relay in the center display? It ran for 72 hours straight on backup power, keeping \
                    the southern district connected. The operator - Sarah Chen - stayed with it the entire time, \
                    making repairs, managing power. A true hero of the network."
                )
                .with_choice(DialogChoice::new("Amazing story").exit())
                .with_choice(DialogChoice::new("Are there other exhibits?").goto("other_exhibits")));

                // Exhibits branch
                tree.insert("exhibit".to_string(), DialogNode::new(
                    "Our centerpiece is the Winter Storm '19 relay - a legendary piece of equipment. We also \
                    have displays on early mesh protocols, pioneer stories, and technical evolution."
                )
                .with_choice(DialogChoice::new("Tell me about the storm").goto("relay_story"))
                .with_choice(DialogChoice::new("What about the pioneers?").goto("pioneers"))
                .with_choice(DialogChoice::new("Very interesting!").exit()));

                tree.insert("other_exhibits".to_string(), DialogNode::new(
                    "We have sections on early mesh protocols, the evolution of hardware, and profiles of \
                    key network architects. There's also a hands-on area where you can see how old equipment \
                    worked compared to modern systems."
                )
                .with_choice(DialogChoice::new("I'd like to see that").exit())
                .with_choice(DialogChoice::new("Tell me about the pioneers").goto("pioneers")));

                tree.insert("pioneers".to_string(), DialogNode::new(
                    "The mesh pioneers were ordinary people who saw a need and met it with creativity and \
                    determination. They didn't have perfect equipment or unlimited resources - just commitment \
                    to keeping the community connected."
                )
                .with_choice(DialogChoice::new("Inspiring!").exit())
                .with_choice(DialogChoice::new("Who were the heroes?").goto("heroes")));

                tree.insert("heroes".to_string(), DialogNode::new(
                    "Too many to list! Sarah Chen of Storm '19 fame, Marcus Webb who designed the resilient \
                    routing protocols we still use, the Tanaka family who donated land for relay towers... \
                    Each plaque here represents someone who put the network above themselves."
                )
                .with_choice(DialogChoice::new("I should learn more").exit())
                .with_choice(DialogChoice::new("Thanks for sharing").exit()));

                // Research Kit branch - gives items to players
                tree.insert("research_offer".to_string(), DialogNode::new(
                    "Wonderful! We're always looking for field researchers to test historical artifacts \
                    and document their functionality. I can provide you with a research kit containing \
                    some example items from our collection."
                )
                .with_choice(DialogChoice::new("I'd be honored!")
                    .goto("receive_kit")
                    .with_condition(DialogCondition::HasFlag { 
                        flag: "received_research_kit".to_string(), 
                        value: false 
                    }))
                .with_choice(DialogChoice::new("I already received one")
                    .goto("already_received")
                    .with_condition(DialogCondition::HasFlag { 
                        flag: "received_research_kit".to_string(), 
                        value: true 
                    }))
                .with_choice(DialogChoice::new("What's in the kit?").goto("kit_details"))
                .with_choice(DialogChoice::new("No thank you, maybe another time").exit()));

                // Already received kit - alternative message
                tree.insert("already_received".to_string(), DialogNode::new(
                    "Ah yes, of course! You're one of our field researchers. How's the work going? \
                    Have you discovered anything interesting about the artifacts? The museum is always \
                    interested in field reports!"
                )
                .with_choice(DialogChoice::new("Still studying them").exit())
                .with_choice(DialogChoice::new("They're quite fascinating!").exit())
                .with_choice(DialogChoice::new("The teleport stone works perfectly!").exit()));

                tree.insert(
                    "kit_details".to_string(),
                    DialogNode::new(
                        "The research kit includes:\n\
                    • A Healing Potion - demonstrates early medicinal technology\n\
                    • An Ancient Key - shows pre-mesh security systems\n\
                    • A Mystery Box - random reward mechanisms from old games\n\
                    • A Tattered Note - fragment of historical correspondence\n\
                    • A Teleport Stone - experimental transit technology\n\
                    • A Singing Mushroom - bioluminescent communication experiment\n\n\
                    These are working replicas you can use and study!",
                    )
                    .with_choice(
                        DialogChoice::new("I'll take the kit!")
                            .goto("receive_kit")
                            .with_condition(DialogCondition::HasFlag {
                                flag: "received_research_kit".to_string(),
                                value: false,
                            }),
                    )
                    .with_choice(DialogChoice::new("Sounds fascinating!").exit()),
                );

                tree.insert("receive_kit".to_string(), DialogNode::new(
                    "Excellent! Here's your museum research kit. Please document your findings - \
                    these artifacts represent important chapters in our network's history. Use them \
                    wisely and report back if you discover anything interesting!"
                )
                .with_action(DialogAction::GiveItem { 
                    item_id: "example_healing_potion".to_string(), 
                    quantity: 1 
                })
                .with_action(DialogAction::GiveItem { 
                    item_id: "example_ancient_key".to_string(), 
                    quantity: 1 
                })
                .with_action(DialogAction::GiveItem { 
                    item_id: "example_mystery_box".to_string(), 
                    quantity: 1 
                })
                .with_action(DialogAction::GiveItem { 
                    item_id: "example_quest_clue".to_string(), 
                    quantity: 1 
                })
                .with_action(DialogAction::GiveItem { 
                    item_id: "example_teleport_stone".to_string(), 
                    quantity: 1 
                })
                .with_action(DialogAction::GiveItem { 
                    item_id: "example_singing_mushroom".to_string(), 
                    quantity: 1 
                })
                .with_action(DialogAction::SetFlag { 
                    flag: "received_research_kit".to_string(), 
                    value: true 
                })
                .with_choice(DialogChoice::new("Thank you, Dr. Reeves!").exit())
                .with_choice(DialogChoice::new("I'll take good care of these").exit()));

                curator.dialog_tree = tree;
                store.put_npc(curator)?;
                updated += 1;
            }
        }
        Err(e) => {
            eprintln!(
                "Warning: Could not load museum_curator for dialogue seeding: {}",
                e
            );
        }
    }

    Ok(updated)
}

/// Create example trigger objects for Phase 9 testing
///
/// This creates 6 example objects demonstrating all trigger types:
/// - Healing Potion (OnUse with consume + heal)
/// - Ancient Key (OnLook with quest check, OnUse unlocks exit)
/// - Mystery Box (OnPoke with random chance)
/// - Quest Clue (OnLook with dynamic description)
/// - Teleport Stone (OnUse teleports player)
/// - Singing Mushroom (OnEnter ambient message)
pub fn create_example_trigger_objects(now: DateTime<Utc>) -> Vec<ObjectRecord> {
    let mut objects = Vec::new();

    // 1. Healing Potion - OnUse trigger with consume() and heal()
    let mut healing_potion = ObjectRecord {
        id: "example_healing_potion".to_string(),
        name: "Healing Potion".to_string(),
        description: "A crystal vial filled with glowing red liquid. When used, it heals wounds and vanishes.".to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 1,
        currency_value: CurrencyAmount::default(),
        value: 50,
        takeable: true,
        usable: true,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    healing_potion.actions.insert(
        ObjectTrigger::OnUse,
        "message(\"The potion glows brightly as you drink it!\") && heal(50) && consume()"
            .to_string(),
    );
    objects.push(healing_potion);

    // 2. Ancient Key - OnLook with quest check, OnUse unlocks exit
    let mut ancient_key = ObjectRecord {
        id: "example_ancient_key".to_string(),
        name: "Ancient Key".to_string(),
        description: "A tarnished brass key with mysterious runes etched along its length."
            .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 1,
        currency_value: CurrencyAmount::default(),
        value: 100,
        takeable: true,
        usable: true,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    ancient_key.actions.insert(
        ObjectTrigger::OnLook,
        "has_quest(\"cryptkeeper_quest\") ? message(\"The runes glow faintly - you recognize them from your quest!\") : message(\"The runes are indecipherable.\")".to_string()
    );
    ancient_key.actions.insert(
        ObjectTrigger::OnUse,
        "message(\"The key turns in an invisible lock...\") && unlock_exit(\"north\")".to_string(),
    );
    objects.push(ancient_key);

    // 3. Mystery Box - OnPoke with random chance
    let mut mystery_box = ObjectRecord {
        id: "example_mystery_box".to_string(),
        name: "Mystery Box".to_string(),
        description: "A wooden box covered in question marks. It rattles when poked.".to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 5,
        currency_value: CurrencyAmount::default(),
        value: 25,
        takeable: true,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    mystery_box.actions.insert(
        ObjectTrigger::OnPoke,
        "random_chance(50) ? message(\"🎁 A small treat pops out!\") : message(\"💨 The box releases a puff of dust.\")".to_string()
    );
    objects.push(mystery_box);

    // 4. Quest Clue - OnLook with dynamic description
    let mut quest_clue = ObjectRecord {
        id: "example_quest_clue".to_string(),
        name: "Tattered Note".to_string(),
        description: "A yellowed piece of parchment with faded ink.".to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 1,
        currency_value: CurrencyAmount::default(),
        value: 5,
        takeable: true,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    quest_clue.actions.insert(
        ObjectTrigger::OnLook,
        "message(\"The note reads: 'Meet me at the old gazebo when the moon is high. - M'\")"
            .to_string(),
    );
    objects.push(quest_clue);

    // 5. Teleport Stone - OnUse teleports player
    let mut teleport_stone = ObjectRecord {
        id: "example_teleport_stone".to_string(),
        name: "Teleport Stone".to_string(),
        description: "A smooth black stone that hums with magical energy.".to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 2,
        currency_value: CurrencyAmount::default(),
        value: 500,
        takeable: true,
        usable: true,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    teleport_stone.actions.insert(
        ObjectTrigger::OnUse,
        "message(\"✨ The stone flashes brilliantly!\") && teleport(\"old_towne_square\")"
            .to_string(),
    );
    objects.push(teleport_stone);

    // 6. Singing Mushroom - OnEnter ambient message
    let mut singing_mushroom = ObjectRecord {
        id: "example_singing_mushroom".to_string(),
        name: "Singing Mushroom".to_string(),
        description: "A peculiar purple mushroom that hums softly to itself.".to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 1,
        currency_value: CurrencyAmount::default(),
        value: 10,
        takeable: true,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    singing_mushroom.actions.insert(
        ObjectTrigger::OnEnter,
        "message(\"🍄 The mushroom chimes a cheerful tune as you enter the room!\")".to_string(),
    );
    objects.push(singing_mushroom);

    // 7. Basic Knife - Merchant item for sale
    let basic_knife = ObjectRecord {
        id: "vendor_basic_knife".to_string(),
        name: "Basic Knife".to_string(),
        description: "A simple but sturdy knife with a wooden handle. Perfect for cutting rope, \
preparing food, or general utility tasks. The blade is sharp and well-maintained."
            .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 1,
        currency_value: CurrencyAmount::decimal(15),
        value: 15,
        takeable: true,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(basic_knife);

    // 8. Signal Booster - Merchant item for sale (purchasable version of the craftable item)
    let signal_booster = ObjectRecord {
        id: "vendor_signal_booster".to_string(),
        name: "Signal Booster".to_string(),
        description: "A professional-grade signal booster manufactured by Mira's family workshop. \
Extends mesh network range by 50% and improves signal clarity in weak areas. Essential equipment \
for wilderness exploration. The copper coils are wrapped with precision, and the circuit board \
bears the family mark of quality."
            .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 3,
        currency_value: CurrencyAmount::decimal(50),
        value: 50,
        takeable: true,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(signal_booster);

    objects
}

/// Create content population objects for the expanded world (Phase 1 of content implementation)
///
/// This creates interactive objects placed throughout the 16-room world including:
/// - Rumor Board (relay_tavern): Read-only bulletin board with quest hints
/// - Diagnostic Panel (repeater_tower): Interactive equipment for quest
/// - Northern Array (repeater_tower): Antenna for observation and diagnostics
/// - Carved Symbols (ancient_grove): Four trees with ancient carvings for puzzle
/// - Crafting Bench (workshop_district): Shows crafting recipes
/// - Crafting Materials (various): Wire, scrap metal, components for crafting system
pub fn create_content_objects(now: DateTime<Utc>) -> Vec<ObjectRecord> {
    let mut objects = Vec::new();

    // 1. Rumor Board (relay_tavern) - Social hub information board
    let rumor_board = ObjectRecord {
        id: "rumor_board".to_string(),
        name: "Rumor Board".to_string(),
        description:
            "A large cork board mounted on the tavern wall, covered in handwritten notes, \
sketches of signal patterns, and cryptic messages. Several notes mention 'unusual readings from \
the Grove' and 'Old Graybeard needs help at the tower'. A faded map shows the Repeater Tower \
to the north and marks something called 'Ancient Grove' to the east."
                .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 100, // Heavy, wall-mounted
        currency_value: CurrencyAmount::default(),
        value: 0,
        takeable: false,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(rumor_board);

    // 2. Diagnostic Panel (repeater_tower) - Quest objective equipment
    let diagnostic_panel = ObjectRecord {
        id: "diagnostic_panel".to_string(),
        name: "Diagnostic Panel".to_string(),
        description: "A wall-mounted control panel with numerous status LEDs, signal strength meters, \
and diagnostic readouts. A large red button labeled 'RUN DIAGNOSTICS' invites interaction. The display \
currently shows all systems in green: 'TOWER STATUS: NOMINAL | SIGNAL QUALITY: 94% | POWER: OPTIMAL'. \
USE this panel to run the full diagnostic sequence.".to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 50,
        currency_value: CurrencyAmount::default(),
        value: 0,
        takeable: false,
        usable: true, // Can USE for diagnostics
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(diagnostic_panel);

    // 3. Northern Array (repeater_tower) - Directional antenna
    let northern_array = ObjectRecord {
        id: "northern_array".to_string(),
        name: "Northern Array".to_string(),
        description: "A large directional antenna mounted on a swivel base, pointing north toward \
Pine Ridge Trail. The antenna has signal strength indicators showing moderate activity. A small \
panel displays 'NORTH SECTOR: 78% OPTIMAL' in green LEDs. The mounting bolts show signs of recent \
maintenance."
            .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 200, // Very heavy equipment
        currency_value: CurrencyAmount::default(),
        value: 0,
        takeable: false,
        usable: true, // Can be used for diagnostics
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(northern_array);

    // 4. Carved Symbols - Ancient Grove (4 trees with carvings)

    // Oak Tree Symbols
    let carved_symbols_oak = ObjectRecord {
        id: "carved_symbols_oak".to_string(),
        name: "Oak Tree Carvings".to_string(),
        description: "Ancient carvings etched into the massive oak trunk. The primary symbol is a \
circle with radiating lines - like a sun, or perhaps a signal broadcast pattern. The grooves are \
worn smooth by centuries of weather, but the design is still clear. Around it are smaller symbols \
that might be letters or numbers in an old script."
            .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 255, // Immovable (part of tree)
        currency_value: CurrencyAmount::default(),
        value: 0,
        takeable: false,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(carved_symbols_oak);

    // Elm Tree Symbols
    let carved_symbols_elm = ObjectRecord {
        id: "carved_symbols_elm".to_string(),
        name: "Elm Tree Carvings".to_string(),
        description: "The elm bears a different symbol: three wavy lines stacked vertically, like \
waves or perhaps signal frequencies. The carving is deeper here, as if the maker wanted to emphasize \
this one. Moss has grown in the grooves, giving the symbol a green glow in the dappled sunlight.".to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 255,
        currency_value: CurrencyAmount::default(),
        value: 0,
        takeable: false,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(carved_symbols_elm);

    // Willow Tree Symbols
    let carved_symbols_willow = ObjectRecord {
        id: "carved_symbols_willow".to_string(),
        name: "Willow Tree Carvings".to_string(),
        description:
            "On the willow's bark: a triangular symbol with a dot at each point, connected \
by curved lines. It resembles a network topology diagram - nodes and connections. The willow's \
drooping branches frame the symbol, creating an almost shrine-like atmosphere."
                .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 255,
        currency_value: CurrencyAmount::default(),
        value: 0,
        takeable: false,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(carved_symbols_willow);

    // Ash Tree Symbols
    let carved_symbols_ash = ObjectRecord {
        id: "carved_symbols_ash".to_string(),
        name: "Ash Tree Carvings".to_string(),
        description:
            "The ash tree shows the final symbol: a spiral that winds inward to a central \
point. Unlike the others, this one has a small hollow at the spiral's center, as if something was \
once placed there. The spiral draws your eye inward, making you feel both calm and alert."
                .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 255,
        currency_value: CurrencyAmount::default(),
        value: 0,
        takeable: false,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(carved_symbols_ash);

    // 5. Crafting Bench (workshop_district)
    let crafting_bench = ObjectRecord {
        id: "crafting_bench".to_string(),
        name: "Crafting Bench".to_string(),
        description:
            "A sturdy workbench made of scarred oak, covered with tools, wire spools, and \
organized bins of components. A hand-written recipe card is pinned to the wall:\n\n\
📋 CRAFTING RECIPES:\n\
  • Signal Booster: 1 wire + 1 scrap metal\n\
  • Basic Antenna: 2 wire + 1 basic component\n\n\
The bench has a vice, soldering iron, wire cutters, and various other tools. Everything is neatly \
organized and well-maintained. A sign reads: 'USE CRAFT <recipe_name> to create items'."
                .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 255,
        currency_value: CurrencyAmount::default(),
        value: 0,
        takeable: false,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(crafting_bench);

    // 6. Crafting Materials (takeable items scattered across locations)

    // Wire Spools (multiple locations)
    let wire_spool_1 = ObjectRecord {
        id: "wire_spool_1".to_string(),
        name: "Wire Spool".to_string(),
        description: "A small spool of insulated copper wire, perfect for making signal equipment."
            .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 1,
        currency_value: CurrencyAmount::decimal(5),
        value: 5,
        takeable: true,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(wire_spool_1);

    let wire_spool_2 = ObjectRecord {
        id: "wire_spool_2".to_string(),
        name: "Wire Spool".to_string(),
        description: "A small spool of insulated copper wire, perfect for making signal equipment."
            .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 1,
        currency_value: CurrencyAmount::decimal(5),
        value: 5,
        takeable: true,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(wire_spool_2);

    // Scrap Metal pieces
    let scrap_metal_1 = ObjectRecord {
        id: "scrap_metal_1".to_string(),
        name: "Scrap Metal".to_string(),
        description:
            "A piece of salvaged metal housing from old equipment. Still useful for crafting."
                .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 2,
        currency_value: CurrencyAmount::decimal(3),
        value: 3,
        takeable: true,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(scrap_metal_1);

    let scrap_metal_2 = ObjectRecord {
        id: "scrap_metal_2".to_string(),
        name: "Scrap Metal".to_string(),
        description:
            "A piece of salvaged metal housing from old equipment. Still useful for crafting."
                .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 2,
        currency_value: CurrencyAmount::decimal(3),
        value: 3,
        takeable: true,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(scrap_metal_2);

    // Basic Components
    let basic_component_1 = ObjectRecord {
        id: "basic_component_1".to_string(),
        name: "Basic Component".to_string(),
        description:
            "A salvaged circuit board with useful capacitors and resistors still attached."
                .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 1,
        currency_value: CurrencyAmount::decimal(10),
        value: 10,
        takeable: true,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(basic_component_1);

    // 7. Torch (takeable light source for dark areas)
    let torch = ObjectRecord {
        id: "torch".to_string(),
        name: "Torch".to_string(),
        description: "A sturdy wooden torch wrapped with oil-soaked cloth. When lit, it provides \
reliable illumination for exploring dark spaces. The flame flickers steadily, casting dancing \
shadows on nearby surfaces. Essential equipment for venturing into the maintenance tunnels."
            .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 1,
        currency_value: CurrencyAmount::decimal(10),
        value: 10,
        takeable: true,
        usable: true, // Can USE to light/extinguish
        actions: std::collections::HashMap::new(),
        flags: vec![ObjectFlag::LightSource],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(torch);

    // 8. Additional Crafting Materials for recipes

    // Copper Wire (used in signal_booster and basic_antenna recipes)
    let copper_wire = ObjectRecord {
        id: "copper_wire".to_string(),
        name: "Copper Wire".to_string(),
        description:
            "High-quality copper wire suitable for antenna construction and signal routing. \
The wire is flexible yet durable, with excellent conductivity properties. Essential for advanced \
crafting projects."
                .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 1,
        currency_value: CurrencyAmount::decimal(8),
        value: 8,
        takeable: true,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(copper_wire);

    // Antenna Rod (used in signal_booster and basic_antenna recipes)
    let antenna_rod = ObjectRecord {
        id: "antenna_rod".to_string(),
        name: "Antenna Rod".to_string(),
        description: "A telescoping metal rod designed for antenna construction. When extended, \
it can serve as an effective radiating element for mesh signals. The rod has calibration marks \
for optimal frequency tuning."
            .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 2,
        currency_value: CurrencyAmount::decimal(15),
        value: 15,
        takeable: true,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(antenna_rod);

    // Crystal Shard (rare material for advanced crafting)
    let crystal_shard = ObjectRecord {
        id: "crystal_shard".to_string(),
        name: "Crystal Shard".to_string(),
        description: "A translucent crystal fragment with unusual electromagnetic properties. \
When held near active mesh equipment, it seems to resonate faintly. Local legends claim these \
crystals were used in ancient communication rituals."
            .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 1,
        currency_value: CurrencyAmount::decimal(50),
        value: 50,
        takeable: true,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(crystal_shard);

    // Signal Capacitor (advanced electronic component)
    let signal_capacitor = ObjectRecord {
        id: "signal_capacitor".to_string(),
        name: "Signal Capacitor".to_string(),
        description: "A high-capacity electrolytic capacitor designed for RF signal filtering. \
The component is labeled with technical specifications and has a slight blue glow from its \
charge indicator. Used in advanced signal processing circuits."
            .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 1,
        currency_value: CurrencyAmount::decimal(25),
        value: 25,
        takeable: true,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(signal_capacitor);

    // ==================== PHASE 4 QUEST OBJECTS ====================

    // CIPHER QUEST SYMBOLS (Phase 4.2) - Must be examined in order: Spring, Summer, Autumn, Winter

    let cipher_spring = ObjectRecord {
        id: "cipher_spring".to_string(),
        name: "Spring Glyph".to_string(),
        description:
            "A stone tablet carved with an intricate symbol representing spring: sprouting \
seedlings emerging from soil, surrounded by dewdrops and young leaves. The carving style matches \
ancient pre-mesh artifacts. Small text below reads: 'GROWTH - The First Signal'."
                .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 50,
        currency_value: CurrencyAmount::default(),
        value: 0,
        takeable: false,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(cipher_spring);

    let cipher_summer = ObjectRecord {
        id: "cipher_summer".to_string(),
        name: "Summer Glyph".to_string(),
        description:
            "A stone tablet showing the summer symbol: a blazing sun with strong, bold rays \
reaching in all directions. The carving depicts maximum energy and reach. Text reads: 'STRENGTH - \
The Broadcast Peak'."
                .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 50,
        currency_value: CurrencyAmount::default(),
        value: 0,
        takeable: false,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(cipher_summer);

    let cipher_autumn = ObjectRecord {
        id: "cipher_autumn".to_string(),
        name: "Autumn Glyph".to_string(),
        description: "The autumn tablet features falling leaves arranged in a spiral pattern, \
suggesting transformation and change. The leaves seem to form data packets flowing through a network. \
Text reads: 'CHANGE - The Signal Adapts'.".to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 50,
        currency_value: CurrencyAmount::default(),
        value: 0,
        takeable: false,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(cipher_autumn);

    let cipher_winter = ObjectRecord {
        id: "cipher_winter".to_string(),
        name: "Winter Glyph".to_string(),
        description: "The final tablet shows a snowflake's perfect geometry - six symmetric branches, \
each subdividing into smaller patterns. Beneath it: bare trees storing energy underground. Text reads: \
'REST - The Silent Network Awaits'.".to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 50,
        currency_value: CurrencyAmount::default(),
        value: 0,
        takeable: false,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(cipher_winter);

    // LOST ARTIFACT QUEST GLYPHS (Phase 4.2 - Epic Quest)

    let ruins_glyph_alpha = ObjectRecord {
        id: "ruins_glyph_alpha".to_string(),
        name: "Alpha Glyph".to_string(),
        description:
            "A weathered stone pillar bearing the first glyph: a simple upward arrow with \
three horizontal lines beneath it. This represents 'TRANSMIT'. The stone is worn but the carving \
remains deep and clear."
                .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 100,
        currency_value: CurrencyAmount::default(),
        value: 0,
        takeable: false,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(ruins_glyph_alpha);

    let ruins_glyph_beta = ObjectRecord {
        id: "ruins_glyph_beta".to_string(),
        name: "Beta Glyph".to_string(),
        description:
            "The second pillar shows a circle with radiating waves - the universal symbol \
for 'RECEIVE'. Moss grows in the carved grooves, giving it an eerie green glow in dim light."
                .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 100,
        currency_value: CurrencyAmount::default(),
        value: 0,
        takeable: false,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(ruins_glyph_beta);

    let ruins_glyph_gamma = ObjectRecord {
        id: "ruins_glyph_gamma".to_string(),
        name: "Gamma Glyph".to_string(),
        description: "The third glyph depicts two circles connected by a curved line - 'RELAY'. \
This symbol appears on ancient communication equipment throughout the mesh."
            .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 100,
        currency_value: CurrencyAmount::default(),
        value: 0,
        takeable: false,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(ruins_glyph_gamma);

    let ruins_glyph_delta = ObjectRecord {
        id: "ruins_glyph_delta".to_string(),
        name: "Delta Glyph".to_string(),
        description: "The final pillar bears a complex mandala: multiple circles interconnected in a \
perfect network pattern. This is 'UNITY' - the goal of all communication. The carving is so intricate \
it seems to shimmer as you examine it.".to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 100,
        currency_value: CurrencyAmount::default(),
        value: 0,
        takeable: false,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(ruins_glyph_delta);

    // LIGHT SOURCE OBJECTS (Phase 4.3 - Dark Navigation)

    let lantern = ObjectRecord {
        id: "lantern".to_string(),
        name: "LED Lantern".to_string(),
        description:
            "A modern LED lantern powered by a small battery pack. Provides bright, steady \
illumination for navigating dark spaces. Much more reliable than torches and won't run out during \
exploration. Has adjustable brightness settings."
                .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 2,
        currency_value: CurrencyAmount::decimal(50),
        value: 50,
        takeable: true,
        usable: true,
        actions: std::collections::HashMap::new(),
        flags: vec![ObjectFlag::LightSource],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(lantern);

    let glowstick = ObjectRecord {
        id: "glowstick".to_string(),
        name: "Chemical Glowstick".to_string(),
        description: "A bendable plastic tube containing chemical compounds that glow when activated. \
Provides soft green illumination for several hours. Not as bright as a lantern, but lightweight and \
reliable. Commonly used by tunnel maintenance crews.".to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 1,
        currency_value: CurrencyAmount::decimal(5),
        value: 5,
        takeable: true,
        usable: true,
        actions: std::collections::HashMap::new(),
        flags: vec![ObjectFlag::LightSource],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(glowstick);

    // CRAFTING MATERIALS & RECIPES (Phase 4.4)

    // Additional crafting components beyond basic ones already defined
    let crystal_oscillator = ObjectRecord {
        id: "crystal_oscillator".to_string(),
        name: "Crystal Oscillator".to_string(),
        description:
            "A precision electronic component - a small quartz crystal that oscillates at \
an exact frequency. Essential for advanced signal processing equipment. Rare and valuable."
                .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 1,
        currency_value: CurrencyAmount::decimal(200),
        value: 200,
        takeable: true,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(crystal_oscillator);

    let power_cell = ObjectRecord {
        id: "power_cell".to_string(),
        name: "Power Cell".to_string(),
        description: "A rechargeable battery cell used to power portable equipment. Still holds a \
charge. Can be used in crafting or sold for credits."
            .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 2,
        currency_value: CurrencyAmount::decimal(75),
        value: 75,
        takeable: true,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(power_cell);

    let circuit_board = ObjectRecord {
        id: "circuit_board".to_string(),
        name: "Circuit Board".to_string(),
        description:
            "A bare printed circuit board with copper traces ready for component placement. \
Used in advanced crafting projects. The traces form an elegant pattern of interconnected pathways."
                .to_string(),
        owner: ObjectOwner::World,
        created_at: now,
        weight: 1,
        currency_value: CurrencyAmount::decimal(100),
        value: 100,
        takeable: true,
        usable: false,
        actions: std::collections::HashMap::new(),
        flags: vec![],
        locked: false,
        clone_depth: 0,
        clone_source_id: None,
        clone_count: 0,
        created_by: "world".to_string(),
        ownership_history: vec![],
        schema_version: OBJECT_SCHEMA_VERSION,
    };
    objects.push(circuit_board);

    objects
}

/// Create content population NPCs for the expanded world (Phase 2 of content implementation)
///
/// This creates 4 NPCs with full dialogue trees placed throughout the world:
/// - Old Graybeard (repeater_tower): Tech mentor, quest giver for tower_diagnostics
/// - Barkeep Mira (relay_tavern): Social hub host, rumor dispenser
/// - Old Elm (west_residential): Mystical elder, quest giver for grove_mystery
/// - Tinker Brass (workshop_district): Crafting mentor, quest giver for first_craft
pub fn create_content_npcs(now: DateTime<Utc>) -> Vec<crate::tmush::types::NpcRecord> {
    use crate::tmush::types::{DialogChoice, DialogNode, NpcRecord};
    use std::collections::HashMap;

    let mut npcs = Vec::new();

    // 1. OLD GRAYBEARD - Repeater Tower technician and quest giver
    let mut old_graybeard = NpcRecord::new(
        "old_graybeard",
        "Old Graybeard",
        "Tower Technician",
        "An elderly man with a weathered face and kind eyes. His beard is steel-gray and neatly \
trimmed, and his hands bear the calluses of decades working with equipment. He wears a faded \
technician's vest covered in pockets, each holding tools, wire snippets, and diagnostic devices. \
Despite his age, his movements are precise and purposeful as he tends to the tower equipment.",
        "repeater_tower",
    );

    let mut graybeard_tree = HashMap::new();

    // Greeting node
    graybeard_tree.insert(
        "greeting".to_string(),
        DialogNode::new(
            "Ah, a visitor! Not many folks make the climb up here. I'm Graybeard - been keeping \
this tower running for nigh on forty years now. What brings you up to my domain?",
        )
        .with_choice(DialogChoice::new("What is this place?").goto("about_tower"))
        .with_choice(DialogChoice::new("Can I help with anything?").goto("quest_offer"))
        .with_choice(DialogChoice::new("Just looking around").exit()),
    );

    // About the tower
    graybeard_tree.insert("about_tower".to_string(), DialogNode::new(
        "This is the Old Towne Repeater Tower - backbone of our mesh network for the northern sector. \
That big antenna there keeps us connected to Pine Ridge and beyond. Without proper maintenance, the \
whole region loses connectivity. It's vital work."
    )
    .with_choice(DialogChoice::new("How does it work?").goto("how_it_works"))
    .with_choice(DialogChoice::new("Sounds important. Can I help?").goto("quest_offer"))
    .with_choice(DialogChoice::new("Fascinating! I should go now").exit()));

    // Technical explanation
    graybeard_tree.insert("how_it_works".to_string(), DialogNode::new(
        "The tower receives signals from the mesh below and amplifies them for long-distance relay. \
We've got three main arrays - north, east, and west - each covering different sectors. The diagnostic \
panel monitors signal strength, and we do regular checks by climbing up to inspect the antennas \
directly. It's simple technology, but reliable when properly maintained."
    )
    .with_choice(DialogChoice::new("Could you teach me?").goto("quest_offer"))
    .with_choice(DialogChoice::new("Thanks for explaining!").exit()));

    // Quest offer
    graybeard_tree.insert("quest_offer".to_string(), DialogNode::new(
        "Well now, that's mighty kind of you! Truth is, I could use a hand with today's diagnostics. \
My knees aren't what they used to be, and climbing the tower ladder takes me twice as long these days. \
If you're willing to help, I can walk you through the procedure. It's a good way to learn how \
these systems work!"
    )
    .with_choice(DialogChoice::new("I'd be happy to help!").goto("quest_accept"))
    .with_choice(DialogChoice::new("What would I need to do?").goto("quest_details"))
    .with_choice(DialogChoice::new("Maybe another time").exit()));

    // Quest details
    graybeard_tree.insert("quest_details".to_string(), DialogNode::new(
        "The routine is straightforward: First, check the diagnostic panel down here on the ground \
level - just USE it and it'll run through its self-tests. Then, climb UP the ladder to the \
upper platform where the antennas are mounted. From up there, check the northern array's \
alignment. That's the whole procedure! Should take you maybe ten minutes."
    )
    .with_choice(DialogChoice::new("Sounds easy enough! I'll do it").goto("quest_accept"))
    .with_choice(DialogChoice::new("Let me think about it").exit()));

    // Quest acceptance
    graybeard_tree.insert("quest_accept".to_string(), DialogNode::new(
        "Excellent! I'll log you as today's assistant. Here's what you need to know: Start with the \
diagnostic panel right here - give it a USE command. Then climb UP to the upper platform. Once \
you're up there, check out that northern array. When you're done, come back and tell me \
what you found. I'll be here tending to the equipment."
    )
    .with_choice(DialogChoice::new("Got it! I'll get started").exit()));

    old_graybeard.dialog_tree = graybeard_tree;
    old_graybeard.created_at = now;
    npcs.push(old_graybeard);

    // 2. BARKEEP MIRA - Relay Tavern social hub host
    let mut barkeep_mira = NpcRecord::new(
        "barkeep_mira",
        "Barkeep Mira",
        "Tavern Keeper",
        "A cheerful woman in her thirties with auburn hair tied back in a practical ponytail. She \
wears a sturdy apron over comfortable clothes, and moves behind the bar with the efficiency of \
someone who's done this for years. Her eyes are bright and observant - she seems to know \
everyone who walks through the door and hear every conversation in the room.",
        "relay_tavern",
    );

    let mut mira_tree = HashMap::new();

    // Greeting
    mira_tree.insert("greeting".to_string(), DialogNode::new(
        "Welcome to the Relay Tavern! Pull up a stool and catch your breath. I'm Mira - I keep this \
place running and the stories flowing. What can I do for you today?"
    )
    .with_choice(DialogChoice::new("What's the latest news?").goto("rumors"))
    .with_choice(DialogChoice::new("Tell me about this place").goto("about_tavern"))
    .with_choice(DialogChoice::new("Who are the interesting people around here?").goto("people"))
    .with_choice(DialogChoice::new("Just passing through, thanks!").exit()));

    // Rumors and news
    mira_tree.insert("rumors".to_string(), DialogNode::new(
        "Oh, you know how it goes - always something happening in Old Towne! Check out the rumor board \
on the wall there if you want the full stories. But between you and me... Old Graybeard at the tower \
has been worried about something. And folks who venture into that grove to the east come back with \
strange tales about symbols and feelings. If you're looking for adventure, those might be good places \
to start!"
    )
    .with_choice(DialogChoice::new("Tell me more about the tower").goto("tower_rumors"))
    .with_choice(DialogChoice::new("What's this about a grove?").goto("grove_rumors"))
    .with_choice(DialogChoice::new("Thanks for the tips!").exit()));

    // Tower rumors
    mira_tree.insert("tower_rumors".to_string(), DialogNode::new(
        "The Repeater Tower is north of here - head to the North Gate, then take the Pine Ridge Trail. \
Old Graybeard maintains it all by himself, bless him. He's getting on in years though, and I think \
he'd appreciate help from someone young and spry like yourself. He's good people - knows more about \
mesh networking than anyone alive."
    )
    .with_choice(DialogChoice::new("What about the grove?").goto("grove_rumors"))
    .with_choice(DialogChoice::new("I'll check it out. Thanks!").exit()));

    // Grove rumors
    mira_tree.insert("grove_rumors".to_string(), DialogNode::new(
        "The Ancient Grove is east of the Museum - through the Forest Path. It's a beautiful spot, \
very peaceful... but there's something else there too. Old Elm, who lives over in the residential lane, \
has been fascinated by it for years. Says there are symbols carved in the trees that predate the mesh \
network. If you're curious about mysteries, you should talk to them."
    )
    .with_choice(DialogChoice::new("Interesting! What about the tower?").goto("tower_rumors"))
    .with_choice(DialogChoice::new("I'll keep that in mind. Thanks!").exit()));

    // About the tavern
    mira_tree.insert("about_tavern".to_string(), DialogNode::new(
        "The Relay Tavern has been here almost as long as the mesh network itself. We're the social \
heart of Old Towne - where people come to unwind, share stories, and hear the latest news. That board \
on the wall there is always full of notices, rumors, and requests for help. Consider it your community \
bulletin board!"
    )
    .with_choice(DialogChoice::new("What's the latest news?").goto("rumors"))
    .with_choice(DialogChoice::new("Who are the interesting locals?").goto("people"))
    .with_choice(DialogChoice::new("Nice place! I'll look around").exit()));

    // Local people
    mira_tree.insert("people".to_string(), DialogNode::new(
        "Oh, where do I start? There's Old Graybeard at the Repeater Tower - salt of the earth, that one. \
Old Elm over in the residential lane, always pondering mysteries. Tinker Brass down in the Workshop \
District if you need anything made or fixed. And of course, Mayor Thompson at City Hall handles all \
the official business. They're all good folk!"
    )
    .with_choice(DialogChoice::new("Tell me the latest rumors").goto("rumors"))
    .with_choice(DialogChoice::new("Thanks for the introductions!").exit()));

    barkeep_mira.dialog_tree = mira_tree;
    barkeep_mira.created_at = now;
    npcs.push(barkeep_mira);

    // 3. OLD ELM - Mystical elder and grove mystery quest giver
    let mut old_elm = NpcRecord::new(
        "old_elm",
        "Old Elm",
        "Village Elder",
        "An ageless figure wrapped in layers of comfortable, earth-toned clothing. Their face is \
lined with years of wisdom, and their eyes hold a knowing gleam. They move slowly but deliberately, \
as if always aware of the flow of time around them. A wooden pendant carved with an intricate symbol \
hangs around their neck. There's something calming about their presence, like being near an ancient tree.",
        "west_residential",
    );

    let mut elm_tree = HashMap::new();

    // Greeting
    elm_tree.insert("greeting".to_string(), DialogNode::new(
        "Ah, hello there, young one. I am called Old Elm, and I have walked these paths longer than \
most can remember. The mesh network connects our devices, yes... but there are older connections, \
deeper patterns. What draws you to speak with me today?"
    )
    .with_choice(DialogChoice::new("Tell me about the old connections").goto("old_patterns"))
    .with_choice(DialogChoice::new("I heard you know about the grove").goto("grove_intro"))
    .with_choice(DialogChoice::new("Just saying hello").exit()));

    // Old patterns philosophy
    elm_tree.insert("old_patterns".to_string(), DialogNode::new(
        "Before radio waves and digital signals, humans found other ways to connect - through stories, \
through symbols, through shared understanding. The Ancient Grove to the east remembers those times. \
The trees there bear marks that speak to something older than our modern world. I have studied them \
for many years."
    )
    .with_choice(DialogChoice::new("What do the symbols mean?").goto("symbols_meaning"))
    .with_choice(DialogChoice::new("Can I help you understand them?").goto("quest_offer"))
    .with_choice(DialogChoice::new("Fascinating perspective").exit()));

    // Symbols meaning
    elm_tree.insert("symbols_meaning".to_string(), DialogNode::new(
        "That is the question I have pondered! I believe they represent concepts we still use today: \
broadcast, frequency, connection, convergence. The ancients who carved them understood these principles \
in their own way. But there is an order to the symbols - a sequence that unlocks understanding. \
I am close to solving it, but my eyes grow dim and my steps grow slow."
    )
    .with_choice(DialogChoice::new("I could help you!").goto("quest_offer"))
    .with_choice(DialogChoice::new("Good luck with your research").exit()));

    // Grove introduction
    elm_tree.insert("grove_intro".to_string(), DialogNode::new(
        "Ah yes, the Ancient Grove. A sacred place, though many have forgotten why. Four great trees \
stand in a circle there - oak, elm, willow, and ash. Each bears a symbol carved deep into its bark. \
I believe these symbols, observed in the proper order, reveal a truth about our connection to this land."
    )
    .with_choice(DialogChoice::new("What's the proper order?").goto("symbols_meaning"))
    .with_choice(DialogChoice::new("Can I help you study them?").goto("quest_offer"))
    .with_choice(DialogChoice::new("Sounds mysterious!").exit()));

    // Quest offer
    elm_tree.insert("quest_offer".to_string(), DialogNode::new(
        "You would do this for an old seeker of truth? I am grateful. The task is simple in action, \
though profound in meaning: Visit the Ancient Grove - east from the Museum, through the Forest Path. \
Find the four trees and EXAMINE their carvings carefully. The order matters: oak, elm, willow, ash. \
Observe them in sequence and see what understanding comes to you."
    )
    .with_choice(DialogChoice::new("I'll do it!").goto("quest_accept"))
    .with_choice(DialogChoice::new("Why that specific order?").goto("sequence_explanation"))
    .with_choice(DialogChoice::new("Let me think about it").exit()));

    // Sequence explanation
    elm_tree.insert("sequence_explanation".to_string(), DialogNode::new(
        "The oak represents strength and foundation - the broadcast source. The elm speaks of community \
and connection - the network itself. The willow shows flexibility and flow - the adaptability of signals. \
The ash, finally, represents wisdom and convergence - all signals meeting in understanding. This is the \
natural flow of communication, ancient and eternal."
    )
    .with_choice(DialogChoice::new("Beautiful! I'll observe them for you").goto("quest_accept"))
    .with_choice(DialogChoice::new("Interesting philosophy").exit()));

    // Quest acceptance
    elm_tree.insert("quest_accept".to_string(), DialogNode::new(
        "Thank you, seeker. May your journey bring understanding. Remember: the Ancient Grove lies east \
from the Museum. EXAMINE each tree's carvings in sequence - oak, elm, willow, ash. When you have seen \
them all, return and share what you have learned. Safe travels."
    )
    .with_choice(DialogChoice::new("I'll return soon").exit()));

    old_elm.dialog_tree = elm_tree;
    old_elm.created_at = now;
    npcs.push(old_elm);

    // 4. TINKER BRASS - Workshop District crafting mentor
    let mut tinker_brass = NpcRecord::new(
        "tinker_brass",
        "Tinker Brass",
        "Master Craftsperson",
        "A sturdy person of indeterminate age, with muscular arms and clever fingers. They wear heavy \
work gloves pushed back on their wrists, and a leather apron studded with pockets containing tools of \
every description. Their eyes are sharp and assessing, always evaluating what things are made of and \
how they might be improved. A pair of magnifying goggles sits pushed up on their forehead.",
        "workshop_district",
    );

    let mut brass_tree = HashMap::new();

    // Greeting
    brass_tree.insert("greeting".to_string(), DialogNode::new(
        "Well hello there! Welcome to my workshop. I'm Brass - I build things, fix things, and teach \
folks how to do both. Always nice to see a new face around here. What brings you down to the district?"
    )
    .with_choice(DialogChoice::new("What do you make here?").goto("about_crafting"))
    .with_choice(DialogChoice::new("Can you teach me to craft?").goto("quest_offer"))
    .with_choice(DialogChoice::new("Just exploring").exit()));

    // About crafting
    brass_tree.insert("about_crafting".to_string(), DialogNode::new(
        "Oh, all sorts of things! Signal boosters, antennas, relay equipment - anything the mesh network \
needs. See that bench over there? That's where the magic happens. It's not really magic though - just \
understanding materials, having the right tools, and following proven recipes. Anyone can learn it with \
proper instruction!"
    )
    .with_choice(DialogChoice::new("Could you teach me?").goto("quest_offer"))
    .with_choice(DialogChoice::new("Sounds complicated").exit()));

    // Quest offer
    brass_tree.insert("quest_offer".to_string(), DialogNode::new(
        "You want to learn? Excellent! I love teaching enthusiastic students. Tell you what - I'll walk \
you through making your first piece of equipment. We'll start with something simple but useful: a \
signal booster. It's the perfect beginner project. You'll need to gather some materials first, though. \
Are you ready to get started?"
    )
    .with_choice(DialogChoice::new("Yes! What do I need?").goto("quest_accept"))
    .with_choice(DialogChoice::new("What exactly will I learn?").goto("learning_details"))
    .with_choice(DialogChoice::new("Maybe later").exit()));

    // Learning details
    brass_tree.insert("learning_details".to_string(), DialogNode::new(
        "Great question! First, you'll learn about materials - what wire is good for, why we use scrap \
metal for housing, how components work. Then, you'll practice gathering resources - they're scattered \
around town. Finally, you'll use the crafting bench to combine materials following a recipe. By the \
end, you'll have a working signal booster AND the knowledge to craft more items on your own!"
    )
    .with_choice(DialogChoice::new("Perfect! Let's do it").goto("quest_accept"))
    .with_choice(DialogChoice::new("Sounds like a lot").exit()));

    // Quest acceptance
    brass_tree.insert("quest_accept".to_string(), DialogNode::new(
        "Wonderful! Here's your first lesson: A signal booster requires one wire spool and one piece \
of scrap metal. You can find these materials around town - check the tunnel maintenance areas, the \
workshop district, and other places where equipment is stored. Once you have both items, bring them \
back here and use the CRAFT command at the bench. I'll be here when you're ready!"
    )
    .with_choice(DialogChoice::new("I'll gather the materials now!").exit()));

    tinker_brass.dialog_tree = brass_tree;
    tinker_brass.created_at = now;
    npcs.push(tinker_brass);

    npcs
}

/// Create content population quests for the expanded world (Phase 3 of content implementation)
///
/// This creates 4 quests that provide guided gameplay through the new content:
/// - tower_diagnostics: Technical puzzle teaching tower navigation
/// - grove_mystery: Observation puzzle with symbol sequence
/// - tunnel_salvage: Dark navigation and resource collection
/// - first_craft: Crafting tutorial quest
pub fn create_content_quests(now: DateTime<Utc>) -> Vec<crate::tmush::types::QuestRecord> {
    use crate::tmush::types::{CurrencyAmount, ObjectiveType, QuestObjective, QuestRecord};

    let mut quests = Vec::new();

    // QUEST 1: Tower Diagnostics (Old Graybeard's quest)
    let mut tower_diagnostics = QuestRecord::new(
        "tower_diagnostics",
        "Tower Diagnostics",
        "Help Old Graybeard perform routine diagnostics on the Repeater Tower. Check the ground-level \
diagnostic panel, climb to the upper platform, and inspect the northern array antenna.",
        "old_graybeard",
        2, // Difficulty: 2/5 (easy-moderate)
    );

    tower_diagnostics.created_at = now;

    tower_diagnostics = tower_diagnostics
        .with_objective(QuestObjective::new(
            "Talk to Old Graybeard",
            ObjectiveType::TalkToNpc {
                npc_id: "old_graybeard".to_string(),
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Use the diagnostic panel",
            ObjectiveType::UseItem {
                item_id: "diagnostic_panel".to_string(),
                target: "self".to_string(),
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Climb to the tower upper platform",
            ObjectiveType::VisitLocation {
                room_id: "repeater_upper".to_string(),
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Inspect the northern array",
            ObjectiveType::UseItem {
                item_id: "northern_array".to_string(),
                target: "self".to_string(),
            },
            1,
        ))
        .with_reward_currency(CurrencyAmount::Decimal { minor_units: 5000 }) // $50 or 500cp
        .with_reward_experience(100)
        .with_reward_item("signal_booster");

    quests.push(tower_diagnostics);

    // QUEST 2: Grove Mystery (Old Elm's quest)
    let mut grove_mystery = QuestRecord::new(
        "grove_mystery",
        "The Grove Mystery",
        "Old Elm believes the Ancient Grove holds secrets about connections that predate the mesh network. \
Visit the grove and examine the four great trees in the proper sequence: oak, elm, willow, ash.",
        "old_elm",
        3, // Difficulty: 3/5 (moderate - requires observation)
    );

    grove_mystery.created_at = now;

    grove_mystery = grove_mystery
        .with_prerequisite("tower_diagnostics") // Must complete tower quest first
        .with_objective(QuestObjective::new(
            "Talk to Old Elm",
            ObjectiveType::TalkToNpc {
                npc_id: "old_elm".to_string(),
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Visit the Ancient Grove",
            ObjectiveType::VisitLocation {
                room_id: "ancient_grove".to_string(),
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Examine the oak tree carvings",
            ObjectiveType::UseItem {
                item_id: "carved_symbols_oak".to_string(),
                target: "examine".to_string(),
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Examine the elm tree carvings",
            ObjectiveType::UseItem {
                item_id: "carved_symbols_elm".to_string(),
                target: "examine".to_string(),
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Examine the willow tree carvings",
            ObjectiveType::UseItem {
                item_id: "carved_symbols_willow".to_string(),
                target: "examine".to_string(),
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Examine the ash tree carvings",
            ObjectiveType::UseItem {
                item_id: "carved_symbols_ash".to_string(),
                target: "examine".to_string(),
            },
            1,
        ))
        .with_reward_currency(CurrencyAmount::Decimal { minor_units: 7500 }) // $75 or 750cp
        .with_reward_experience(150)
        .with_reward_item("ancient_token");

    quests.push(grove_mystery);

    // QUEST 3: Tunnel Salvage (Exploration quest)
    let mut tunnel_salvage = QuestRecord::new(
        "tunnel_salvage",
        "Tunnel Salvage",
        "The maintenance tunnels beneath Old Towne are dark and labyrinthine, but they contain valuable \
salvage materials. Navigate the tunnels and collect useful components for the community.",
        "mayor_thompson", // Given by Mayor as a community service quest
        3, // Difficulty: 3/5 (requires light source, navigation)
    );

    tunnel_salvage.created_at = now;

    tunnel_salvage = tunnel_salvage
        .with_objective(QuestObjective::new(
            "Enter the maintenance tunnels",
            ObjectiveType::VisitLocation {
                room_id: "maintenance_tunnels".to_string(),
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Collect wire spools",
            ObjectiveType::CollectItem {
                item_id: "wire_spool".to_string(),
                count: 2,
            },
            2,
        ))
        .with_objective(QuestObjective::new(
            "Collect scrap metal",
            ObjectiveType::CollectItem {
                item_id: "scrap_metal".to_string(),
                count: 2,
            },
            2,
        ))
        .with_objective(QuestObjective::new(
            "Find a basic component",
            ObjectiveType::CollectItem {
                item_id: "basic_component".to_string(),
                count: 1,
            },
            1,
        ))
        .with_reward_currency(CurrencyAmount::Decimal { minor_units: 6000 }) // $60 or 600cp
        .with_reward_experience(125);

    quests.push(tunnel_salvage);

    // QUEST 4: First Craft (Tinker Brass's tutorial quest)
    let mut first_craft = QuestRecord::new(
        "first_craft",
        "Your First Craft",
        "Tinker Brass wants to teach you the basics of crafting. Gather the materials for a signal booster \
(1 wire spool + 1 scrap metal) and learn to use the crafting bench in the Workshop District.",
        "tinker_brass",
        1, // Difficulty: 1/5 (tutorial quest)
    );

    first_craft.created_at = now;

    first_craft = first_craft
        .with_objective(QuestObjective::new(
            "Talk to Tinker Brass",
            ObjectiveType::TalkToNpc {
                npc_id: "tinker_brass".to_string(),
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Collect a wire spool",
            ObjectiveType::CollectItem {
                item_id: "wire_spool".to_string(),
                count: 1,
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Collect scrap metal",
            ObjectiveType::CollectItem {
                item_id: "scrap_metal".to_string(),
                count: 1,
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Visit the Workshop District",
            ObjectiveType::VisitLocation {
                room_id: "workshop_district".to_string(),
            },
            1,
        ))
        // Note: The actual CRAFT command execution will complete this quest
        .with_reward_currency(CurrencyAmount::Decimal { minor_units: 2500 }) // $25 or 250cp
        .with_reward_experience(75)
        .with_reward_item("crafters_apron"); // Cosmetic reward showing they learned crafting

    quests.push(first_craft);

    // ==================== PHASE 4 QUESTS ====================
    // These quests specifically use Phase 4.2-4.4 mechanics

    // QUEST 5: The Cipher (Phase 4.2 - Symbol Sequence Quest)
    let mut the_cipher = QuestRecord::new(
        "the_cipher",
        "The Cipher",
        "An ancient message has been discovered, encoded in symbols scattered throughout the ancient ruins. \
Examine the symbols in the correct sequence to unlock their meaning. Legends say the order follows \
the cycle of seasons: Spring (growth), Summer (strength), Autumn (change), Winter (rest).",
        "old_elm",
        4, // Difficulty: 4/5 (requires careful observation and correct sequence)
    );

    the_cipher.created_at = now;

    the_cipher = the_cipher
        .with_prerequisite("grove_mystery") // Must complete grove mystery first
        .with_objective(QuestObjective::new(
            "Talk to Old Elm about the cipher",
            ObjectiveType::TalkToNpc {
                npc_id: "old_elm".to_string(),
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Examine symbols in correct sequence",
            ObjectiveType::ExamineSequence {
                object_ids: vec![
                    "cipher_spring".to_string(),
                    "cipher_summer".to_string(),
                    "cipher_autumn".to_string(),
                    "cipher_winter".to_string(),
                ],
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Return to Old Elm with your discovery",
            ObjectiveType::TalkToNpc {
                npc_id: "old_elm".to_string(),
            },
            1,
        ))
        .with_reward_currency(CurrencyAmount::Decimal { minor_units: 10000 }) // $100 or 1000cp
        .with_reward_experience(250)
        .with_reward_item("decoder_lens"); // Special item for future puzzles

    quests.push(the_cipher);

    // QUEST 6: Into the Depths (Phase 4.3 - Dark Navigation Quest)
    let mut into_the_depths = QuestRecord::new(
        "into_the_depths",
        "Into the Depths",
        "The Deep Caverns beneath Old Towne have never been fully explored. They're pitch black - you'll \
need a light source to navigate safely. Rumors speak of a hidden chamber containing pre-mesh artifacts.",
        "old_graybeard",
        4, // Difficulty: 4/5 (requires light source, navigation in darkness)
    );

    into_the_depths.created_at = now;

    into_the_depths = into_the_depths
        .with_prerequisite("tunnel_salvage") // Must complete tunnel salvage first
        .with_objective(QuestObjective::new(
            "Obtain a light source",
            ObjectiveType::ObtainLightSource,
            1,
        ))
        .with_objective(QuestObjective::new(
            "Enter the Deep Caverns",
            ObjectiveType::NavigateDarkRoom {
                room_id: "deep_caverns_entrance".to_string(),
                requires_light: true,
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Navigate to the Sunken Chamber",
            ObjectiveType::NavigateDarkRoom {
                room_id: "sunken_chamber".to_string(),
                requires_light: true,
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Discover the hidden vault",
            ObjectiveType::NavigateDarkRoom {
                room_id: "hidden_vault".to_string(),
                requires_light: true,
            },
            1,
        ))
        .with_reward_currency(CurrencyAmount::Decimal { minor_units: 12000 }) // $120 or 1200cp
        .with_reward_experience(300)
        .with_reward_item("ancient_relay_core"); // Rare crafting material

    quests.push(into_the_depths);

    // QUEST 7: Master Artisan (Phase 4.4 - Crafting Chain Quest)
    let mut master_artisan = QuestRecord::new(
        "master_artisan",
        "Master Artisan",
        "Tinker Brass believes you're ready for advanced crafting techniques. Prove your skill by crafting \
multiple items from scratch: an antenna, a relay module, and finally a complex signal array.",
        "tinker_brass",
        5, // Difficulty: 5/5 (requires extensive material gathering and crafting)
    );

    master_artisan.created_at = now;

    master_artisan = master_artisan
        .with_prerequisite("first_craft") // Must complete basic crafting first
        .with_objective(QuestObjective::new(
            "Craft a basic antenna",
            ObjectiveType::CraftItem {
                item_id: "basic_antenna".to_string(),
                count: 1,
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Craft a relay module",
            ObjectiveType::CraftItem {
                item_id: "relay_module".to_string(),
                count: 1,
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Craft an advanced signal array",
            ObjectiveType::CraftItem {
                item_id: "signal_array_advanced".to_string(),
                count: 1,
            },
            1,
        ))
        .with_reward_currency(CurrencyAmount::Decimal { minor_units: 15000 }) // $150 or 1500cp
        .with_reward_experience(400)
        .with_reward_item("master_crafter_badge"); // Title unlock item

    quests.push(master_artisan);

    // QUEST 8: The Lost Artifact (Combined Phase 4 Mechanics - Epic Quest)
    let mut the_lost_artifact = QuestRecord::new(
        "the_lost_artifact",
        "The Lost Artifact",
        "Legends tell of an ancient communication device hidden in the Forgotten Ruins. To reach it, you must: \
decipher the entrance symbols, navigate the dark passages with a light source, and craft a special key to \
unlock the artifact chamber. This is the ultimate test of your skills.",
        "old_elm",
        5, // Difficulty: 5/5 (requires all Phase 4 mechanics)
    );

    the_lost_artifact.created_at = now;

    the_lost_artifact = the_lost_artifact
        .with_prerequisite("the_cipher") // Must complete cipher quest
        .with_prerequisite("into_the_depths") // Must complete dark navigation quest
        .with_prerequisite("master_artisan") // Must complete crafting mastery
        .with_objective(QuestObjective::new(
            "Find the Forgotten Ruins entrance",
            ObjectiveType::VisitLocation {
                room_id: "forgotten_ruins_entrance".to_string(),
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Decipher the entrance sequence",
            ObjectiveType::ExamineSequence {
                object_ids: vec![
                    "ruins_glyph_alpha".to_string(),
                    "ruins_glyph_beta".to_string(),
                    "ruins_glyph_gamma".to_string(),
                    "ruins_glyph_delta".to_string(),
                ],
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Navigate the Dark Passage",
            ObjectiveType::NavigateDarkRoom {
                room_id: "ruins_dark_passage".to_string(),
                requires_light: true,
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Craft the Artifact Chamber Key",
            ObjectiveType::CraftItem {
                item_id: "artifact_chamber_key".to_string(),
                count: 1,
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Enter the Artifact Chamber",
            ObjectiveType::VisitLocation {
                room_id: "artifact_chamber".to_string(),
            },
            1,
        ))
        .with_objective(QuestObjective::new(
            "Collect the Ancient Communication Device",
            ObjectiveType::CollectItem {
                item_id: "ancient_comm_device".to_string(),
                count: 1,
            },
            1,
        ))
        .with_reward_currency(CurrencyAmount::Decimal { minor_units: 50000 }) // $500 or 5000cp - epic reward
        .with_reward_experience(1000)
        .with_reward_item("legendary_mesh_artifact"); // Legendary item

    quests.push(the_lost_artifact);

    quests
}

/// Create faction definitions for the reputation system (Phase 5)
pub fn create_factions() -> Vec<crate::tmush::types::FactionRecord> {
    use crate::tmush::types::FactionRecord;

    let mut factions = Vec::new();

    // FACTION 1: Old Towne Citizens
    let old_towne = FactionRecord::new(
        "old_towne",
        "Old Towne Citizens",
        "The community of Old Towne Mesh. Gaining their trust grants access to community resources, \
housing discounts, and insider information. They value reliability and community service.",
    )
    .with_npc("mayor_thompson")
    .with_npc("city_clerk")
    .with_quest("welcome_towne")
    .with_quest("market_exploration")
    .with_benefit("Friendly", "10% discount at all Old Towne shops")
    .with_benefit("Honored", "Access to community storage, priority housing")
    .with_benefit("Revered", "Old Towne Ambassador title, town hall access");

    factions.push(old_towne);

    // FACTION 2: Tinkers Guild
    let tinkers = FactionRecord::new(
        "tinkers",
        "Tinkers Guild",
        "Master craftspeople and engineers who build and maintain the mesh network. Members gain access \
to advanced crafting recipes, rare materials, and technical knowledge.",
    )
    .with_npc("tinker_brass")
    .with_npc("old_graybeard")
    .with_quest("first_craft")
    .with_quest("tower_diagnostics")
    .with_quest("master_artisan")
    .with_benefit("Friendly", "Access to basic crafting recipes")
    .with_benefit("Honored", "Advanced crafting recipes, 20% material discount")
    .with_benefit("Revered", "Master Crafter title, legendary recipes, workshop access");

    factions.push(tinkers);

    // FACTION 3: Wanderers League
    let wanderers = FactionRecord::new(
        "wanderers",
        "Wanderers League",
        "Explorers, adventurers, and those who map the unknown. They reward discovery and bravery \
with maps, equipment, and tales of distant places.",
    )
    .with_quest("network_explorer")
    .with_quest("tunnel_salvage")
    .with_quest("into_the_depths")
    .with_benefit("Friendly", "Access to exploration maps, 10% faster travel")
    .with_benefit(
        "Honored",
        "Pathfinder title, rare light sources, danger sense",
    )
    .with_benefit("Revered", "Legendary Explorer title, teleportation access");

    factions.push(wanderers);

    // FACTION 4: Merchants Coalition
    let traders = FactionRecord::new(
        "traders",
        "Merchants Coalition",
        "The economic backbone of Old Towne. Traders value commerce and fair dealing. Good standing \
grants better prices, rare goods access, and trading privileges.",
    )
    .with_benefit("Friendly", "5% better buying/selling prices")
    .with_benefit("Honored", "15% better prices, access to rare item auctions")
    .with_benefit("Revered", "Merchant Prince title, private trading network");

    factions.push(traders);

    // FACTION 5: Scholars Circle
    let scholars = FactionRecord::new(
        "scholars",
        "Scholars Circle",
        "Keepers of history and knowledge. They study the old world and seek to understand the mysteries \
of the ancient communication networks. Rewards include rare lore, decryption keys, and historical artifacts.",
    )
    .with_npc("old_elm")
    .with_quest("grove_mystery")
    .with_quest("the_cipher")
    .with_quest("the_lost_artifact")
    .with_benefit("Friendly", "Access to library, basic lore knowledge")
    .with_benefit("Honored", "Lorekeeper title, cipher solving bonuses, artifact appraisal")
    .with_benefit("Revered", "Master Scholar title, access to restricted archives");

    factions.push(scholars);

    // FACTION 6: Underground Network
    let underground = FactionRecord::new(
        "underground",
        "Underground Network",
        "A secretive group operating in the tunnels and shadows. They deal in information, black market \
goods, and alternative solutions. Gaining their trust is difficult but rewarding.",
    )
    .with_benefit("Friendly", "Access to tunnel shortcuts, black market contacts")
    .with_benefit("Honored", "Shadow Runner title, stealth bonuses, information network")
    .with_benefit("Revered", "Underground Legend title, master fence access, safe houses");

    factions.push(underground);

    factions
}
