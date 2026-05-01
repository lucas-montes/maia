# Roadmap / TODO

The idea is to have an assistant that keep tracks of important things for me, reminds me of tasks and meetings and allows me to keep track of my notes. The current interation will be fairly simple and dumb as they are CRUD operations. Later on I will integrate a little more AI (LLMs) into it to have a more "clever" assistant.

---
### Health & Nutrition
- **Calorie Tracking**
  - Should have an entry in the database to track information about the products (nutriment information). We should be able to create a relationship with the product itself and the purchases made from a receipt. I also want to have a relationship with the source of the product (the supermarket and the url or picture of the nutriments information, for intermarche for example we could parse an url to get nutritional information)
  - The daemon will read a specific dir (that should be set from a config file or maybe directly from the database? we need to think about this). It will check when a new receipt (an image of a receipt currenlty). It will send the image to an AI model to extract the information, save the information in the database in the purchases table to track products bought.
  -Then from the cli I should be able to add some meal that I ate to the calory tracking table.
  - Be able to create a plan for nutrition (I have the thing in excel I need to bring the formula here), and set goals. The goals should be coupled with the goals tables of the [section](###Task-&-Calendar-Management)

- **Exercise**
  - Have a table to keep track of the exercise set and the progress, the number of sets, weights and son on.


### Finance & Budgeting
- **Expense & Income Tracking**
  - I should have a few tables to keep track of expenses, income, savings and investments.
  - The daemon should be able to read from a dir different files to parse and save movements like investments from firstrade, or bank statements (I also should look into API to get information, trade212 maybe has an API to get information from).
  - Update expenses and budget based on parsed data from the purchases from the receipts in the [section](###Health-&-Nutrition).
  - Have a way to run simulations to verify financial decisions (cost of living in a remote country, inflation, investments, etc...)


### System Integration
- **Waybar Integration**
  - Add support for Waybar (either calling the CLI or directly in the daemon) to display status, tasks, or notifications.
- **Remote Storage**
  - Add configuration and logic to save files to remote storage for backup and tracking.

### Task & Calendar Management
- **Task Management**
  - Add tasks and goals section: ability to add, track, and complete tasks and goals.
  - Request and display tasks to do / next meeting.
  - Fix notifications for when tasks finish, are due, or start.
- **Google Calendar Integration**
  - Connect to Google Calendar to fetch and display upcoming meetings.

### Personal tracking
- **Meetings**
  - Having a way to record meetings.



### Data Visualization
- **UI/Streaming**
  - Add a way to stream data (using tauri or a web UI) to visualize calories, expenses, tasks, etc.

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
