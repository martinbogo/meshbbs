# MeshBBS Unified Web Interface

## Single-Page Experience Architecture

The MeshBBS web interface has been redesigned as a comprehensive Single-Page Experience (SPE) that consolidates all functionality into one flowing, nature-inspired interface.

## Core Philosophy

- **Natural Design**: Earth tones, organic curves, warm interactions
- **Cloudflare-Inspired Layout**: Persistent header, collapsible sidebar, modular cards
- **Role-Aware**: Interface adapts based on user permissions
- **Smooth Transitions**: No page reloads, smooth scrolling navigation
- **Modular Cards**: Clear hierarchy, task-oriented controls

## Interface Structure

### Persistent Header
- **Logo**: MeshBBS branding with 🌿 icon
- **Tab Navigation**: Home | Messages | Games | Admin (role-dependent)
- **User Display**: Shows username and role badge

### Collapsible Side Navigation
Content changes based on active tab:
- **Home**: Overview, Activity sections
- **Messages**: Quick actions (New Message, Refresh)
- **Games**: Game module links
- **Admin**: Administration sections (Users, Messages, Audit, System)

### Main Content Area
Adapts to accommodate side navigation collapse

## Tabs & Sections

### 1. Home Tab
**Purpose**: Welcome dashboard and system overview

**Sections:**
- **Overview**: 4 stat cards (Users, Messages, Topics, Online)
- **Activity**: Real-time activity timeline

**API Endpoints Used:**
- `GET /api/stats` - System statistics
- `GET /api/activity/feed?limit=10` - Recent activity

### 2. Messages Tab
**Purpose**: Browse and participate in community discussions

**Layout:**
- **Left Panel**: Topic list with message counts
- **Right Panel**: Messages for selected topic

**Features:**
- Click topic to view messages
- Role-based moderation controls (Pin, Delete)
- Real-time formatting

**API Endpoints Used:**
- `GET /api/topics` - List all topics
- `GET /api/topics/{topic}/messages` - Messages for topic

### 3. Games Tab
**Purpose**: Access game modules

**Game Cards:**
- **TinyHack** ⚔️ - Dungeon crawler adventure
- **TinyMUSH** 🏰 - Multi-user shared environment  
- **Fortune** 🔮 - Random wisdom and quotes
- **Magic 8-Ball** 🎱 - Question answering

**Features:**
- Visual game cards with icons
- Status badges (Enabled/Disabled)
- Click to launch (interfaces to be implemented)

### 4. Admin Tab (Level 6+)
**Purpose**: System administration and moderation

**Sections:**
- **User Management**: View/edit user accounts
- **Message Moderation**: Pin/delete/edit messages
- **Audit Log**: Track administrative actions
- **System Settings**: Configuration options

**Visibility:** Only shown to users with level ≥ 6

## Design System

### Colors (Nature-Inspired)
- **Forest**: `--color-forest-deep` (primary), `--color-forest-medium`, `--color-forest-light`, `--color-forest-mist`
- **Stone**: `--color-stone-darkest` (text) through `--color-stone-lightest` (background)
- **Accents**: `--color-accent-amber`, `--color-accent-forest`

### Spacing (Fibonacci)
- 6px, 10px, 16px, 26px, 42px, 68px
- Variables: `--spacing-xxs` through `--spacing-xxl`

### Border Radius (Organic)
- **Never 8px** - use 12px, 16px, 24px, 32px
- Variables: `--radius-sm` (12px), `--radius-md` (16px), `--radius-lg` (24px), `--radius-xl` (32px)

### Shadows (Soft Depth)
- `--shadow-subtle`: Light lift for cards
- `--shadow-lifted`: Hover state elevation
- `--shadow-distant`: Deep context layers

### Transitions (Natural Motion)
- `--transition-swift`: 200ms - Quick interactions
- `--transition-smooth`: 350ms - Content changes
- `--transition-gentle`: 500ms - Page transitions

## Component Library

### Stat Cards
```html
<div class="stat-card">
    <div class="stat-value">42</div>
    <div class="stat-label">Active Users</div>
    <div class="stat-change positive">↑ Growing</div>
</div>
```

### Activity Timeline
```html
<div class="activity-item">
    <div class="activity-icon">💬</div>
    <div class="activity-content">
        <div class="activity-description">...</div>
        <div class="activity-time">5m ago</div>
    </div>
</div>
```

### Game Cards
```html
<div class="game-card">
    <div class="game-card-banner">🎮</div>
    <div class="game-card-content">
        <div class="game-card-title">Game Name</div>
        <div class="game-card-description">...</div>
        <div class="game-card-status">...</div>
    </div>
</div>
```

### Message Cards
```html
<div class="message-card">
    <div class="message-header">
        <span class="message-author">Username</span>
        <span class="message-time">5m ago</span>
    </div>
    <div class="message-content">...</div>
    <div class="message-actions">...</div>
</div>
```

## Responsive Behavior

### Desktop (>1024px)
- Side navigation visible by default
- Multi-column card grids
- Full header navigation

### Tablet (768px - 1024px)
- Side navigation collapsed by default
- Toggle button to show/hide
- Adjusted card grids

### Mobile (<768px)
- Side navigation hidden
- Single column layouts
- Compact header
- Touch-optimized interactions

## JavaScript State Management

### State Object
```javascript
const state = {
    currentTab: 'home',
    currentUser: null,
    sideNavCollapsed: false
};
```

### Key Functions
- `initApp()` - Initialize application
- `switchTab(tabName)` - Change active tab
- `updateSideNav(tabName)` - Update sidebar content
- `toggleSideNav()` - Show/hide sidebar
- `loadStats()` - Fetch system statistics
- `loadActivity()` - Fetch activity feed
- `loadTopics()` - Fetch topic list
- `loadMessages(topicName)` - Fetch messages
- `formatRelativeTime(timestamp)` - Human-readable time

### Auto-Refresh
- Activity feed refreshes every 30 seconds when Home tab active
- Stats update with activity

## Future Enhancements

### Phase 2: Game Interfaces
- [ ] Embedded TinyHack terminal interface
- [ ] TinyMUSH WebSocket connection
- [ ] Fortune display modal
- [ ] 8-Ball interactive UI

### Phase 3: Admin Features
- [ ] User edit modal with role selector
- [ ] Message moderation bulk actions
- [ ] Audit log filtering and search
- [ ] System configuration interface

### Phase 4: User Features
- [ ] User profile page
- [ ] Message composition modal
- [ ] Reply threading
- [ ] Notifications system

### Phase 5: Polish
- [ ] Loading skeletons for all content
- [ ] Error state handling
- [ ] Offline support
- [ ] Progressive Web App (PWA)

## File Structure

```
static/
├── app.html              # Unified SPE interface (NEW)
├── admin-design.css      # Design system
├── dashboard.html        # Old dashboard (backup)
├── users.html           # Old users page (backup)
├── messages.html        # Old messages page (backup)
├── audit.html           # Old audit page (backup)
└── index.html           # Login page
```

## Migration Path

### For Users
1. Access `http://localhost:9885/app.html`
2. All functionality in one interface
3. No more page navigation needed

### For Developers
1. All old pages remain as reference/backup
2. New development focuses on `app.html`
3. API endpoints unchanged
4. Design system in `admin-design.css` used throughout

## Testing Checklist

- [x] Create unified SPE structure
- [x] Implement tab switching
- [x] Add collapsible sidebar
- [x] Load stats from API
- [x] Load activity feed
- [x] Load topics and messages
- [x] Add game module cards
- [x] Role-based visibility (admin tab)
- [x] Responsive layout
- [ ] Test all API integrations live
- [ ] Test user role transitions
- [ ] Test responsive breakpoints
- [ ] Implement game interfaces
- [ ] Add user edit functionality
- [ ] Add message composition

## API Requirements

### Existing (Working)
✅ `GET /api/stats`
✅ `GET /api/activity/feed?limit=N`
✅ `GET /api/topics`
✅ `GET /api/topics/{topic}/messages`
✅ `GET /api/users` (admin)

### Future Needs
- `POST /api/messages` - Compose new message
- `PUT /api/messages/{id}/pin` - Pin/unpin message
- `DELETE /api/messages/{id}` - Delete message
- `PUT /api/users/{username}/level` - Update user level
- `GET /api/games/{game}/status` - Check game availability

## Design Principles Applied

✅ **SPE Philosophy**: One continuous scrolling environment
✅ **Cloudflare-Inspired**: Persistent header, collapsible sidebar, modular cards
✅ **Nature-Inspired**: Earth tones, organic curves, warm neutrals
✅ **No 8px Radius**: Using 12px, 16px, 24px, 32px
✅ **Filled Icons**: Using emoji icons throughout
✅ **Task-Oriented**: "View messages", "Manage users", "Play games"
✅ **Contextual Help**: Side navigation provides context
✅ **Natural Motion**: Smooth transitions, organic animations
✅ **Soft Shadows**: Multi-layer depth without harshness
✅ **Role-Aware**: Admin features hidden from regular users

## Success Metrics

- **Zero Page Reloads**: All navigation via smooth transitions
- **Fast Loading**: Initial stats load <500ms
- **Smooth Scrolling**: 60fps transitions
- **Mobile-Friendly**: Touch-optimized for all devices
- **Accessible**: Keyboard navigation supported
- **Coherent**: Consistent design throughout

---

**Status**: Initial implementation complete
**Version**: 1.0.0
**Date**: October 18, 2025
**Ready For**: User testing and feedback
