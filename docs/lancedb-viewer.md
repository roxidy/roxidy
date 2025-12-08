# Viewing LanceDB Data

Qbit stores sidecar context capture data in a LanceDB database located at:

```
~/.qbit/sidecar/sidecar.lance
```

## Viewing Options

### 1. Python CLI (Recommended)

The most reliable method. Note: Python package versions differ from Rust crate versions.
Qbit uses Rust crate version 0.22.3, which corresponds to Python package ~0.22.

```bash
pip install lancedb pandas
```

```python
import lancedb

db = lancedb.connect("~/.qbit/sidecar/sidecar.lance")
print("Tables:", db.table_names())

# View events
events = db.open_table("events")
print(events.to_pandas())

# View checkpoints
checkpoints = db.open_table("checkpoints")
print(checkpoints.to_pandas())

# View sessions
sessions = db.open_table("sessions")
print(sessions.to_pandas())
```

### 2. Lance Data Viewer (Docker)

A web-based viewer. Note: requires matching LanceDB versions.

```bash
# Mount the sidecar.lance directory directly (not the parent)
docker run --rm -p 8080:8080 \
  -v ~/.qbit/sidecar/sidecar.lance:/data:ro \
  ghcr.io/gordonmurray/lance-data-viewer:lancedb-0.24.3
```

Then open http://localhost:8080 in your browser.

**Version note**: Qbit uses LanceDB Rust crate 0.22.3. The viewer uses Python package versions
(0.24.3, 0.16.0, 0.3.4). Try the latest viewer first - if you get compatibility errors, you may
need to build a custom image:

```bash
git clone https://github.com/lance-format/lance-data-viewer
cd lance-data-viewer
docker build -f docker/Dockerfile --build-arg LANCEDB_VERSION=0.22 -t lance-viewer:0.22 .
docker run --rm -p 8080:8080 -v ~/.qbit/sidecar/sidecar.lance:/data:ro lance-viewer:0.22
```

### 3. DuckDB Integration

LanceDB integrates with DuckDB for SQL queries:

```bash
pip install duckdb lancedb
```

```python
import duckdb
import lancedb

db = lancedb.connect("~/.qbit/sidecar/sidecar.lance")
events = db.open_table("events")

# Use DuckDB to query with SQL
result = duckdb.query("""
    SELECT event_type, COUNT(*) as count
    FROM events
    GROUP BY event_type
    ORDER BY count DESC
""").to_df()
print(result)
```

## Database Schema

The sidecar database contains the following tables:

| Table | Description |
|-------|-------------|
| `events` | All captured events (tool calls, responses, reasoning, etc.) |
| `checkpoints` | Periodic summaries of session activity |
| `sessions` | Session metadata (start time, workspace, status) |

### Event Types

| Type | Description |
|------|-------------|
| `user_message` | User input to the agent |
| `assistant_message` | Agent responses |
| `tool_request` | Tool call requests |
| `tool_result` | Tool execution results (success/failure) |
| `reasoning` | Agent reasoning traces |
| `error` | Error events |

## Resources

- [LanceDB Documentation](https://lancedb.github.io/lancedb/)
- [Lance Data Viewer GitHub](https://github.com/lance-format/lance-data-viewer)
- [LanceDB Python API Reference](https://lancedb.github.io/lancedb/python/)
- [Lance Data Viewer Blog Post](https://lancedb.com/blog/lance-data-viewer/)
