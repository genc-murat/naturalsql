# NaturalSQL

A modern desktop SQL client that lets you query MySQL databases using **natural language**, powered by a local LLM (Ollama).

## Features

- **Natural Language Queries**: Type your question in plain English, get SQL back
- **Schema Browser**: Visual tree view of tables and columns
- **SQL Preview**: Edit the generated SQL before executing
- **Results Table**: Sortable, paginated table with NULL highlighting
- **Dark/Light Theme**: Persistent theme preference
- **Local LLM**: Uses Ollama — no API keys, no cloud, fully local

## Tech Stack

- **Backend**: Rust + Tauri v2
- **Frontend**: React + TypeScript + Tailwind CSS
- **Database**: MySQL 5.6+ (via `mysql_async`)
- **Schema Cache**: SQLite (via `rusqlite`)
- **LLM**: Ollama (`gemma3:1b` by default, configurable)
- **Results**: TanStack Table (sorting, pagination)

## Prerequisites

1. **Rust**: Install from [rustup.rs](https://rustup.rs)
2. **Node.js**: v18+
3. **Ollama**: Install from [ollama.com](https://ollama.com) and pull the model:
   ```bash
   ollama pull gemma3:1b
   ```
4. **MySQL**: Running MySQL 5.6+ instance

## Getting Started

```bash
# Install dependencies
npm install

# Run in development mode (starts Vite dev server + Tauri app)
npm run tauri dev

# Build for production
npm run tauri build
```

## Usage

1. **Connect**: Enter your MySQL connection string in the header:
   ```
   mysql://username:password@localhost:3306/database_name
   ```

2. **Cache Schema**: Click "Cache Schema" in the sidebar to introspect the database structure

3. **Query**: Type a natural language question, e.g.:
   - "show all users who registered last month"
   - "list the top 10 orders by total amount"
   - "count how many products are in each category"

4. **Review SQL**: The generated SQL appears in the preview — edit if needed

5. **Execute**: Click "Execute" or press `Ctrl+Enter` to run the query

## Architecture

```
┌─────────────────────────────────────────────────┐
│                   React Frontend                 │
│  ┌──────────────┐ ┌────────────┐ ┌────────────┐ │
│  │ Connection   │ │ Schema     │ │ Query      │ │
│  │ Panel        │ │ Browser    │ │ Editor     │ │
│  └──────────────┘ └────────────┘ └────────────┘ │
│  ┌────────────────────────────────────────────┐  │
│  │          Results Table                     │  │
│  └────────────────────────────────────────────┘  │
└──────────────────────┬──────────────────────────┘
                       │ Tauri IPC
┌──────────────────────┴──────────────────────────┐
│                   Rust Backend                   │
│  ┌──────────────┐ ┌────────────┐ ┌────────────┐ │
│  │ MySQL        │ │ Schema     │ │ Ollama     │ │
│  │ Connection   │ │ Cache      │ │ Client     │ │
│  └──────────────┘ └────────────┘ └────────────┘ │
└──────────┬──────────────────────────┬───────────┘
           │                          │
     ┌─────┴─────┐              ┌─────┴─────┐
     │  MySQL    │              │  Ollama   │
     │  Server   │              │  Server   │
     └───────────┘              └───────────┘
```

## Configuration

The default Ollama model is `gemma4:e2b`. You can change this by modifying the `model` parameter in the `nl_to_sql` command.

## License

MIT
