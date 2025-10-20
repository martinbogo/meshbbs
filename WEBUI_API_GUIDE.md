# MeshBBS Admin WebUI - API Integration Guide

**Quick Reference for Frontend Developers**

---

## Authentication

### Login
```javascript
POST /api/auth/login
Content-Type: application/json

{
  "username": "sysop",
  "password": "your_password"
}

Response (200 OK):
{
  "token": "eyJhbGciOiJIUzI1NiIs...",
  "username": "sysop",
  "admin_level": 10
}

Response (401 Unauthorized):
{
  "error": "Invalid credentials"
}
```

### Logout
```javascript
POST /api/auth/logout
Authorization: Bearer <token>

Response (200 OK):
{
  "status": "logged_out"
}
```

**Frontend Usage:**
```javascript
// Store token
localStorage.setItem('authToken', token);

// Use in requests
headers: {
  'Authorization': `Bearer ${authToken}`
}

// Clear on logout
localStorage.removeItem('authToken');
```

---

## System Statistics

### Get Stats
```javascript
GET /api/stats

Response (200 OK):
{
  "total_users": 66,
  "users_with_passwords": 45,
  "total_topics": 3,
  "total_messages": 65,
  "unique_message_authors": 42,
  "users_by_role": {
    "1": 50,
    "5": 10,
    "10": 6
  },
  "messages_per_topic": [
    {
      "topic": "general",
      "count": 45
    }
  ]
}
```

**Frontend Usage:**
```javascript
const response = await fetch('/api/stats');
const data = await response.json();

document.getElementById('statUsers').textContent = data.total_users;
document.getElementById('statTopics').textContent = data.total_topics;
document.getElementById('statMessages').textContent = data.total_messages;
document.getElementById('statActive').textContent = data.unique_message_authors;
```

---

## Activity Feed

### Get Recent Activity
```javascript
GET /api/activity/feed?limit=10

Response (200 OK):
{
  "activities": [
    {
      "activity_type": "new_user",
      "description": "User KD7BBC joined",
      "timestamp": 1729283640,
      "metadata": {
        "username": "KD7BBC"
      }
    },
    {
      "activity_type": "message",
      "description": "New message in general by KD7BBC",
      "timestamp": 1729283520,
      "metadata": {
        "topic": "general",
        "author": "KD7BBC"
      }
    }
  ]
}
```

**Activity Types:**
- `new_user` - New user registration
- `message` - New message posted
- `topic` - New topic created
- `login` - User login

**Frontend Usage:**
```javascript
const response = await fetch('/api/activity/feed?limit=10');
const data = await response.json();

data.activities.forEach(activity => {
  const icon = getActivityIcon(activity.activity_type);
  const time = formatTime(activity.timestamp);
  // Render activity item
});

function formatTime(timestamp) {
  const now = Date.now() / 1000;
  const diff = now - timestamp;
  
  if (diff < 60) return 'Just now';
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}
```

---

## User Management

### List All Users
```javascript
GET /api/users
GET /api/users?min_level=5
GET /api/users?max_level=10
GET /api/users?limit=50&offset=0

Response (200 OK):
[
  {
    "username": "KD7BBC",
    "callsign": "KD7BBC",
    "level": 10,
    "message_count": 42,
    "last_login": "2025-10-18T15:30:00Z",
    "node_id": "!a1b2c3d4",
    "password_hash": "..."
  },
  ...
]
```

**Frontend Usage:**
```javascript
const response = await fetch('/api/users');
const users = await response.json();

// Filter by search term
const filtered = users.filter(user => 
  user.callsign.toLowerCase().includes(searchTerm.toLowerCase())
);

// Sort by level
filtered.sort((a, b) => b.level - a.level);
```

### Get Single User
```javascript
GET /api/users/:username

Response (200 OK):
{
  "username": "KD7BBC",
  "callsign": "KD7BBC",
  "level": 10,
  "message_count": 42,
  "last_login": "2025-10-18T15:30:00Z",
  "node_id": "!a1b2c3d4",
  "password_hash": "..."
}
```

### Update User Level
```javascript
PUT /api/users/:username/level
Authorization: Bearer <token>
Content-Type: application/json

{
  "level": 5
}

Response (200 OK):
{
  "status": "updated",
  "username": "KD7BBC",
  "new_level": 5
}
```

**Frontend Usage:**
```javascript
async function updateUserLevel(username, newLevel) {
  const response = await fetch(`/api/users/${username}/level`, {
    method: 'PUT',
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${authToken}`
    },
    body: JSON.stringify({ level: newLevel })
  });
  
  if (!response.ok) {
    const error = await response.json();
    throw new Error(error.error || 'Failed to update');
  }
  
  return response.json();
}
```

---

## Topics & Messages

### List Topics
```javascript
GET /api/topics

Response (200 OK):
[
  {
    "id": "general",
    "name": "General Discussion",
    "message_count": 45,
    "last_message_time": 1729283640
  },
  {
    "id": "tech",
    "name": "Technology",
    "message_count": 20,
    "last_message_time": 1729283500
  }
]
```

### Get Messages in Topic
```javascript
GET /api/topics/:topic/messages
GET /api/topics/:topic/messages?limit=50&offset=0

Response (200 OK):
{
  "topic": "general",
  "messages": [
    {
      "id": "msg_001",
      "author": "KD7BBC",
      "text": "Hello everyone!",
      "timestamp": 1729283640,
      "pinned": false
    },
    ...
  ],
  "total": 45
}
```

### Delete Message
```javascript
DELETE /api/topics/:topic/messages/:id
Authorization: Bearer <token>

Response (200 OK):
{
  "status": "deleted",
  "message_id": "msg_001"
}
```

**Frontend Usage:**
```javascript
async function deleteMessage(topicId, messageId) {
  const response = await fetch(
    `/api/topics/${topicId}/messages/${messageId}`,
    {
      method: 'DELETE',
      headers: {
        'Authorization': `Bearer ${authToken}`
      }
    }
  );
  
  if (!response.ok) {
    throw new Error('Failed to delete message');
  }
  
  return response.json();
}
```

### Pin/Unpin Message
```javascript
PUT /api/topics/:topic/messages/:id/pin
Authorization: Bearer <token>
Content-Type: application/json

{
  "pinned": true
}

Response (200 OK):
{
  "status": "updated",
  "pinned": true
}
```

### Update Message Title
```javascript
PUT /api/topics/:topic/messages/:id/title
Authorization: Bearer <token>
Content-Type: application/json

{
  "title": "New Title"
}

Response (200 OK):
{
  "status": "updated",
  "title": "New Title"
}
```

---

## Audit Log

### Get Audit Entries
```javascript
GET /api/audit/logs
GET /api/audit/logs?action=LOGIN
GET /api/audit/logs?status=failed
GET /api/audit/logs?action=DELETE&status=success
GET /api/audit/logs?page=1&limit=50

Response (200 OK):
{
  "entries": [
    {
      "timestamp": "2025-10-18T15:30:45",
      "action": "LOGIN",
      "username": "sysop",
      "resource": null,
      "ip_address": "192.168.1.100",
      "session_token": "abc123...",
      "status": "success",
      "reason": null
    },
    {
      "timestamp": "2025-10-18T15:31:00",
      "action": "UPDATE",
      "username": "sysop",
      "resource": "user/KD7BBC",
      "ip_address": "192.168.1.100",
      "session_token": "abc123...",
      "status": "success",
      "reason": "level changed from 1 to 5"
    }
  ],
  "total": 234
}
```

**Action Types:**
- `LOGIN` - User login
- `LOGOUT` - User logout
- `VIEW` - Resource viewed
- `UPDATE` - Resource updated
- `DELETE` - Resource deleted
- `CREATE` - Resource created

**Frontend Usage:**
```javascript
async function loadAuditLog(action = '', status = '') {
  let url = '/api/audit/logs?limit=50';
  if (action) url += `&action=${action}`;
  if (status) url += `&status=${status}`;
  
  const response = await fetch(url);
  const data = await response.json();
  
  return data.entries;
}

// Filter by action type
const loginAttempts = await loadAuditLog('LOGIN');

// Filter by status
const failures = await loadAuditLog('', 'failed');

// Combine filters
const failedLogins = await loadAuditLog('LOGIN', 'failed');
```

---

## Error Handling

### Common Error Responses

**401 Unauthorized:**
```json
{
  "error": "Invalid credentials"
}
```

**403 Forbidden:**
```json
{
  "error": "Insufficient permissions"
}
```

**404 Not Found:**
```json
{
  "error": "Resource not found"
}
```

**500 Internal Server Error:**
```json
{
  "error": "Internal server error"
}
```

### Frontend Error Handling Pattern
```javascript
async function apiCall(url, options = {}) {
  try {
    const response = await fetch(url, options);
    
    if (!response.ok) {
      const error = await response.json();
      
      switch (response.status) {
        case 401:
          // Redirect to login
          localStorage.removeItem('authToken');
          window.location.href = '/admin-app.html';
          break;
        
        case 403:
          alert('You do not have permission for this action');
          break;
        
        case 404:
          alert('Resource not found');
          break;
        
        default:
          alert(error.error || 'An error occurred');
      }
      
      throw new Error(error.error || 'Request failed');
    }
    
    return response.json();
  } catch (error) {
    console.error('API Error:', error);
    throw error;
  }
}
```

---

## Best Practices

### 1. Always Include Authorization
```javascript
const headers = {
  'Content-Type': 'application/json',
  'Authorization': `Bearer ${localStorage.getItem('authToken')}`
};
```

### 2. Handle Token Expiration
```javascript
// Check if token exists before making request
if (!authToken) {
  window.location.href = '/admin-app.html';
  return;
}

// Handle 401 responses by redirecting to login
if (response.status === 401) {
  localStorage.removeItem('authToken');
  window.location.reload();
}
```

### 3. Escape HTML in User Content
```javascript
function escapeHtml(text) {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

// Use when displaying user-generated content
messageElement.innerHTML = escapeHtml(message.text);
```

### 4. Debounce Search Inputs
```javascript
let searchTimeout;
searchInput.addEventListener('input', (e) => {
  clearTimeout(searchTimeout);
  searchTimeout = setTimeout(() => {
    performSearch(e.target.value);
  }, 300);
});
```

### 5. Show Loading States
```javascript
async function loadData() {
  const container = document.getElementById('data');
  container.innerHTML = '<div class="loading">Loading...</div>';
  
  try {
    const data = await fetch('/api/data').then(r => r.json());
    renderData(data);
  } catch (error) {
    container.innerHTML = '<div class="error">Failed to load</div>';
  }
}
```

---

## Testing Endpoints with cURL

### Login
```bash
curl -X POST http://localhost:9885/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"sysop","password":"your_password"}'
```

### Get Stats
```bash
curl http://localhost:9885/api/stats
```

### Update User (Authenticated)
```bash
TOKEN="your_token_here"

curl -X PUT http://localhost:9885/api/users/KD7BBC/level \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"level":5}'
```

### Delete Message (Authenticated)
```bash
curl -X DELETE http://localhost:9885/api/topics/general/messages/msg_001 \
  -H "Authorization: Bearer $TOKEN"
```

---

## WebSocket Support (Future)

**Not yet implemented, but planned for real-time updates:**

```javascript
// Connect to WebSocket
const ws = new WebSocket('ws://localhost:9885/api/ws/events');

// Handle events
ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  
  switch(data.type) {
    case 'new_message':
      // Update message list
      break;
    case 'user_updated':
      // Refresh user in table
      break;
    case 'stats_update':
      // Update dashboard stats
      break;
  }
};

// Send heartbeat
setInterval(() => {
  ws.send(JSON.stringify({ type: 'ping' }));
}, 30000);
```

---

**File:** `WEBUI_API_GUIDE.md`  
**Author:** GitHub Copilot  
**Date:** October 18, 2025
