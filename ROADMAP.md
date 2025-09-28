# Roadmap / TODO

---

### System Integration
- **Waybar Integration**
  - Add support for Waybar (either via CLI or directly in the daemon) to display status, tasks, or notifications.

### Task & Calendar Management
- **Task Management**
  - Add tasks and goals section: ability to add, track, and complete tasks and goals.
  - Request and display tasks to do / next meeting.
  - Fix notifications for when tasks finish, are due, or start.
- **Google Calendar Integration**
  - Connect to Google Calendar to fetch and display upcoming meetings.

### Health & Nutrition
- **Calorie Tracking**
  - Track calories from receipts placed in the receipts directory.
  - When the watcher receives a new receipt event, send the image to the AI for parsing.
  - Extract products from the ticket, get or create product entries, and update calorie and cost tracking.
  - Visualize calorie data (see UI section).

### Finance & Budgeting
- **Expense & Income Tracking**
  - Add expense/income tracker and portfolio tracker.
  - Read and parse files from bank and investment statements.
  - Update expenses and budget based on parsed data.
- **Remote Storage**
  - Add configuration and logic to save files to remote storage for backup and tracking.

### Data Visualization
- **UI/Streaming**
  - Add a way to stream data (using ratatui or a web UI) to visualize calories, expenses, tasks, etc.

### Knowledge Base & Browser Integration
- **Chrome Integration**
  - Finish integration with Chrome: listen to browser events, save relevant data, and add it to the knowledge base.
- **Knowledge Base Logic**
  - Implement clever logic to create a knowledge map, similarity search, and advanced querying.

---
