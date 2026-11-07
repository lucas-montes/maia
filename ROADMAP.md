# Roadmap / TODO

The idea is to have an assistant that keep tracks of important things for me, reminds me of tasks and meetings and allows me to keep track of my notes and finances.

---
### Health & Nutrition
- **Calorie Tracking**
  - Parsing receipts should try to match the price of the things with an actual ingredient in the database.
  - We should be able to keep track of the price of a given product, find the source url or the price per kg or something and be able to track the evolution of the price.
  - In the desktop app I want to have more graphics to see the calories eaten by day, week and month along with all the nutriments.

- **Exercise**
  - I want to have in the desktop app more graphics to visualize the evolution of the weights, and reps done by exercise.


### Finance & Budgeting
- **Expense & Income Tracking**
  - Have a way to run simulations to verify financial decisions (cost of living in a remote country, inflation, investments, etc...)
  - fix how statemetns are imported. I want to have first a list of the files that are being parsed and have statements to be approved. once i click on one I get a list of all the entries to insert, and accounts to create if any.


### System Integration
- **Waybar Integration**
  - Add support for Waybar to display status, tasks, or notifications.
- **Notifications Integration**
  - Add support for desktop notifications to display status, tasks, or notifications.
- **Remote Storage**
  - Add configuration and logic to save files to remote storage for backup and tracking.

### Task & Calendar Management
- **Task Management**
  - Request and display tasks to do / next meeting.
  - Have in the desktop app an actual calendar view

- **Google/outlook Calendar Integration**
  - Connect to Google/outlook Calendar to fetch and display upcoming meetings.

### Personal tracking
- **Meetings**
  - Having a way to record meetings.

### Project architecture
- **Components**
  - We should better split each component such as the server, the desktop app, and the chrome extension

### Data Visualization
- **UI**
  - The desktop app needs more graphs and ways to visualize the informations we have

### Knowledge Base & Browser Integration
- **Chrome Integration**
  - Finish integration with Chrome: listen to browser events, save relevant data, and add it to the knowledge base.
- **Knowledge Base Logic**
  - In the config file (or datbase) keep track of where to store notes, bookmarks from the browser and links to read.
  - Implement clever logic to create a knowledge map, similarity search, and advanced querying.
- **Meetings**
  - Having a way to record meetings.
- **Day to day**
  - Maybe have something recording sound and image of my day to day? (this is to do veyr later on).
---
