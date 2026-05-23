# NaturalSQL

A modern desktop SQL client that lets you query MySQL databases using **natural language**, powered by a local LLM (Ollama).

## Features

### Core
- **Natural Language → SQL**: Describe what you want in plain English, get a working SQL query
- **CodeMirror 6 Editor**: Syntax highlighting, auto-complete (schema-aware), multi-cursor, line numbers, bracket matching
- **Schema Browser**: Tree view of databases → tables → columns with type icons and primary key indicators
- **Results Table**: Sortable columns, pagination (50 rows/page), right-click context menu (Copy Value, Copy Column Name)
- **Resizable Panels**: Drag to resize sidebar (databases) and main editor/results area independently
- **Dark/Light Theme**: Persistent theme preference, auto-detected from system, editor themes sync automatically
- **Execution Metrics**: Shows row count, execution time, and affected rows for every query

### Connection Management
- **Multi-Database**: Connect once, browse all databases on the server — no single-DB lock-in
- **Connection Profiles**: Save named connection configurations for quick switching
- **Structured Connection Form**: Host, port, database, username, password — no more typing connection strings
- **Schema Caching**: Per-database schema cache persisted to SQLite — cache once, use forever
- **Auto-Select**: If you specify a database in the connection string, it's auto-selected on connect

## AI-Powered SQL Generation
- **NL → SQL**: Natural language question + schema context → generated SQL via Ollama
  - **Smart Schema Discovery** (tool calling): LLM discovers tables/columns iteratively using function calling — only loads what it needs
  - **Fallback**: If model doesn't support tool calling, automatically uses single-prompt with all cached schemas
  - **Cross-Database JOINs**: LLM can discover tables across multiple databases and write cross-database JOINs
- **Smart Join Builder**: Modal with manual (dropdowns for tables/columns/join type) and AI (natural language description) modes

### AI-Powered SQL Enhancement
- **AI Explain**: Click any SQL query → get a plain-English explanation of what it does
- **Fix with AI**: When a query errors, one click sends SQL + error to LLM → fixed SQL auto-replaces in editor
- **Optimize**: EXPLAIN + LLM analysis → table scan detection, index suggestions, optimized query

### AI-Powered Result Set Actions
Actions that work directly on query results (no new database queries):
- **Smart Aggregation**: Suggests useful COUNT/SUM/AVG/GROUP BY queries
- **Chart Suggestion**: Recommends the best visualization type for your data
- **Pivot Suggestion**: Suggests transposing/pivoting for better readability
- **Report Export**: Generates a natural language summary of the results
- **Follow-up Questions**: Ask anything about the current result set in free text
- **SQL Apply**: Generated SQL from any action can be applied directly to the editor with one click

### Data Analysis Chat
- **Side Panel Chat**: Natural language questions about your data
- **3-Step Pipeline**: LLM generates SQL → executes query → interprets results with context
- **Data Preview**: Shows sample rows alongside the AI's interpretation
- **Suggested SQL**: Collapsible generated SQL with copy button

## Tech Stack

| Layer | Technology |
|---|---|
| **Backend** | Rust + Tauri v2 |
| **Frontend** | React 19 + TypeScript + Vite 7 |
| **Styling** | Tailwind CSS 3.4 (CSS variables for theming) |
| **SQL Editor** | CodeMirror 6 + `@codemirror/lang-sql` |
| **MySQL Driver** | `mysql_async` 0.34 (async) |
| **Schema Cache** | `rusqlite` 0.32 (SQLite, bundled) |
| **HTTP Client** | `reqwest` 0.12 (Ollama API calls) |
| **Results Table** | TanStack Table 8 (sorting, pagination) |
| **Icons** | lucide-react |
| **Resizable Panels** | Custom drag handlers (no dependency) |

## Prerequisites

1. **Rust**: Install from [rustup.rs](https://rustup.rs)
2. **Node.js**: v18+
3. **Ollama**: Install from [ollama.com](https://ollama.com)
   ```bash
   ollama pull gemma4:e2b   # or any model you prefer
   ```
4. **MySQL**: Running MySQL 5.6+ instance

## Getting Started

```bash
# Install dependencies
npm install

# Run in development mode
npm run tauri dev

# Build for production
npm run tauri build
```

## Usage

### 1. Connect
Click the connection badge in the header → fill in Host, Port, Database, Username, Password → Connect.

Or save a **Connection Profile** for one-click reconnecting later.

### 2. Cache Schema
In the sidebar, click **⬇️** next to a database to cache its schema. The ✅ indicates it's cached.

Cache multiple databases to enable cross-database JOIN queries.

### 3. Query
Type a natural language question and press **Translate**:
```
"show all users who registered last month"
"list the top 10 orders by total amount"
"count products in each category"
```

### 4. Review & Edit SQL
The generated SQL appears in the CodeMirror editor. Edit it freely — auto-complete works with your cached schema.

### 5. Execute
Click **Run** or press `Ctrl+Enter`. Results appear below with sorting and pagination.

### 6. Enhance with AI
| Action | Where | What |
|---|---|---|
| **AI Explain** | Toolbar `📖` | Explains SQL in plain English |
| **Fix with AI** | Error bar `🔧` | Fixes syntax/logic errors |
| **Optimize** | Toolbar `⚡` | EXPLAIN analysis + optimization suggestions |
| **Join Builder** | Toolbar `🔗` | Visual or AI-powered JOIN query builder |
| **AI Actions** | Below results | Aggregate, Chart, Pivot, Report, Follow-up |
| **Data Chat** | Header `💬` | Side panel for NL data questions |

### 7. Resize
Drag the divider between sidebar and main area, or between editor and results, to adjust panel sizes.

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                        React Frontend                         │
│  ┌───────────┐ ┌────────────────┐ ┌───────────────────────┐  │
│  │ Connection│ │ Schema Browser │ │ CodeMirror 6 Editor   │  │
│  │ Modal     │ │ (Multi-DB)     │ │ + AI Explain/Fix/     │  │
│  │ Profiles  │ │ + Cache mgmt   │ │   Optimize/Join       │  │
│  └───────────┘ └────────────────┘ └───────────────────────┘  │
│  ┌────────────────────┐  ┌────────────────────────────────┐  │
│  │ Results Table      │  │ Result Actions (AI on results) │  │
│  │ + Context Menu     │  │ + Data Analysis Chat Panel     │  │
│  │ + Execution Time   │  │ + Smart Aggregation/Chart/     │  │
│  └────────────────────┘  │   Pivot/Report/Follow-up       │  │
└──────────────┬───────────┴────────────────────────────────┘  │
               │ Tauri IPC (invoke/handle)                     │
┌──────────────┴───────────────────────────────────────────────┐
│                        Rust Backend                           │
│  ┌─────────────────┐  ┌────────────────┐  ┌───────────────┐  │
│  │ MySQL Connection│  │ Schema         │  │ Ollama Client │  │
│  │ + Pool mgmt     │  │ Introspection  │  │ + NL→SQL      │  │
│  │ + USE database  │  │ + SQLite cache │  │ + AI Explain  │  │
│  └─────────────────┘  └────────────────┘  │ + AI Fix      │  │
│  ┌─────────────────┐  ┌────────────────┐  │ + Optimize    │  │
│  │ Query Execution │  │ Config System  │  │ + Join Builder│  │
│  │ + Timing        │  │ + JSON file    │  │ + Analysis    │  │
│  │ + EXPLAIN       │  │ + Profiles     │  │ + Results AI  │  │
│  └─────────────────┘  └────────────────┘  └───────────────┘  │
└──────────┬──────────────────────────────┬─────────────────────┘
           │                              │
     ┌─────┴─────┐                  ┌─────┴─────┐
     │  MySQL    │                  │  Ollama   │
     │  5.6+     │                  │  Server   │
     └───────────┘                  └───────────┘
```

## Model Compatibility

### Tool Calling (Recommended)
For the best cross-database schema discovery experience, use a model that supports function calling:
- **Google AI / Vertex API**: `gemma-4` with function calling via `ai.google.dev`
- **Ollama**: Models with tool calling support (check [Ollama docs](https://ollama.com/blog/tool-support))

When tool calling is unavailable, NaturalSQL automatically falls back to the single-prompt approach, which loads all cached schemas into the context. This works well for smaller numbers of tables but may struggle with very large schemas.

### Recommended Models
| Model | Size | Tool Calling | Notes |
|---|---|---|---|
| `gemma4:e2b` | ~2B | Via Google API | Fast, good for SQL |
| `llama3.2` | ~3B | Ollama supported | Well-tested |
| `qwen2.5-coder` | ~3B | Ollama supported | Great for code generation |

## Configuration

### LLM Settings
Click the ⚙️ icon in the header to configure:
- **Ollama URL**: Default `http://localhost:11434`
- **Model**: Default `gemma4:e2b`

Configuration is persisted to the app's config directory.

### Connection Profiles
Profiles are stored in the same config file. Each profile saves:
- Name, Host, Port, Database, Username, Password

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+Enter` | Execute current SQL |
| `Enter` (in NL input) | Translate to SQL |
| `Ctrl+Space` (in editor) | Trigger auto-complete |

## License

MIT
