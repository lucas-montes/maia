# Maia Personal Management Tool - Usage Guide

Maia is a comprehensive personal productivity suite with AI-powered features for managing todos, notes, expenses, and calorie tracking.

## Prerequisites

- **For AI Features**: Set the `GEMINI_API_KEY` environment variable with your Google Gemini API key
- **Daemon**: The daemon must be running for CLI commands to work

## Starting the System

1. **Start the daemon** (in one terminal):
   ```bash
   cargo run --package daemon
   ```

2. **Use the CLI** (in another terminal):
   ```bash
   cargo run --bin maia -- <command>
   ```

## Features Overview

### 📝 Todo Management
Organize your tasks with a simple todo system.

**Basic Commands:**
```bash
# Add a new todo
maia todo add --title "Buy groceries" --description "Get milk, bread, and eggs"

# List all todos
maia todo list

# List only pending todos
maia todo list --status pending

# Mark todo as complete
maia todo complete --id 1

# Delete a todo
maia todo delete --id 1
```

### 📄 Notes Management
Create and manage markdown notes stored as files.

**Basic Commands:**
```bash
# Add a new note
maia notes add --title "Meeting Notes" --content "# Project Discussion\n- Review deadlines\n- Assign tasks"

# List all notes
maia notes list

# Search notes
maia notes list --search "project"

# Edit a note
maia notes edit --id 1 --content "Updated content here"

# Delete a note
maia notes delete --id 1
```

**Note Files:**
- Notes are stored in the `./notes/` directory
- Files are named with timestamp and title: `YYYY-MM-DD_HH_MM_SS_SSS_Title.md`
- You can also edit notes directly in your favorite text editor

### 💰 Expense Tracking (AI-Powered)
Upload receipt images to automatically extract expense information.

**Requirements:**
- Set `GEMINI_API_KEY` environment variable
- Receipt images in common formats (JPG, PNG, etc.)

**Basic Commands:**
```bash
# Add expenses from receipt images
maia expenses add --images receipt1.jpg,receipt2.png

# List all expenses
maia expenses list

# List expenses for specific month
maia expenses list --month 2024-07
```

**What the AI extracts:**
- Merchant name and address
- Total amount and currency
- Date of purchase
- Individual items with prices
- Tax information

### 🍎 Calorie Tracking (AI-Powered)
Log food intake and get automatic calorie estimates.

**Basic Commands:**
```bash
# Add food entry (AI estimates calories)
maia calories add --food "apple" --quantity 1

# Add food with image for better accuracy
maia calories add --food "pizza slice" --quantity 2 --image pizza.jpg

# List all calorie entries
maia calories list

# List entries for specific date
maia calories list --date 2024-07-09

# Update quantity for an entry
maia calories update --id 1 --quantity 1.5

# Delete an entry
maia calories delete --id 1
```

**AI Features:**
- Automatic calorie estimation based on food description
- Image analysis for more accurate portion size estimation
- Daily calorie totals and summaries

## Getting Help

Each command and subcommand has detailed help available:

```bash
# Main help
maia --help

# Command help
maia todo --help
maia notes --help
maia expenses --help
maia calories --help

# Subcommand help
maia todo add --help
maia calories add --help
# etc.
```

## Examples

### Daily Workflow Example
```bash
# Morning: Plan your day
maia todo add --title "Finish project report" --description "Complete sections 3-5"
maia todo add --title "Call dentist" --description "Schedule cleaning appointment"

# Lunch: Log your meal
maia calories add --food "chicken salad" --quantity 1 --image lunch.jpg

# Afternoon: Take notes during meeting
maia notes add --title "Team Standup $(date +%Y-%m-%d)" --content "## Agenda\n- Sprint review\n- Blockers\n\n## Action Items\n- Update documentation"

# Evening: Log dinner and expenses
maia calories add --food "salmon with rice" --quantity 1
maia expenses add --images grocery_receipt.jpg

# Review your day
maia todo list --status pending
maia calories list --date $(date +%Y-%m-%d)
```

### Expense Tracking Workflow
```bash
# After shopping, scan receipts
maia expenses add --images receipt_groceries.jpg,receipt_gas.jpg,receipt_restaurant.jpg

# Review monthly expenses
maia expenses list --month $(date +%Y-%m)

# The AI will extract:
# - Store names, addresses
# - Total amounts, tax
# - Individual items purchased
# - Dates and times
```

## Database and Storage

- **Database**: SQLite database stored as `./maia.db`
- **Notes**: Markdown files in `./notes/` directory
- **Socket**: Unix socket at `/tmp/maia.sock` for CLI-daemon communication

## Troubleshooting

**Common Issues:**

1. **"Failed to open socket"**
   - Make sure the daemon is running
   - Check that `/tmp/maia.sock` exists

2. **"AI features require GEMINI_API_KEY"**
   - Set your API key: `export GEMINI_API_KEY=your_key_here`
   - Get an API key from [Google AI Studio](https://makersuite.google.com/app/apikey)

3. **"No such file or directory" for images**
   - Use absolute paths or ensure images are in current directory
   - Check file permissions and formats (JPG, PNG supported)

## Development

**Run tests:**
```bash
cargo test
```

**Build release version:**
```bash
cargo build --release
```

The compiled binaries will be in `target/release/`:
- `target/release/daemon` - The daemon server
- `target/release/maia` - The CLI client
