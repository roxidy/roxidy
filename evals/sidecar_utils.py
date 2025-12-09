"""
Utilities for querying the Qbit sidecar LanceDB database.

The sidecar system captures terminal events, file modifications, and session metadata
into a LanceDB vector database for context retrieval and analysis.

Database schema:
- sessions: Session metadata (id, started_at_ms, ended_at_ms, workspace_path, etc.)
- events: Terminal/file events (id, session_id, timestamp_ms, event_type, content, etc.)
- checkpoints: Periodic session snapshots (id, session_id, timestamp_ms, summary, etc.)

Usage:
    >>> from sidecar_utils import connect_sidecar_db, get_last_session
    >>> db = connect_sidecar_db()
    >>> session = get_last_session(db)
    >>> if session:
    ...     print(f"Last session: {session['id']}")
"""

from pathlib import Path
from typing import Optional

import lancedb
import pandas as pd


# Default sidecar database location
SIDECAR_DB_PATH = Path.home() / ".qbit" / "sidecar" / "sidecar.lance"


def connect_sidecar_db(db_path: Optional[Path] = None) -> lancedb.DBConnection:
    """
    Connect to the sidecar LanceDB database.

    Args:
        db_path: Path to the LanceDB database directory.
                 Defaults to ~/.qbit/sidecar/sidecar.lance

    Returns:
        LanceDB connection object

    Raises:
        FileNotFoundError: If the database path doesn't exist
    """
    path = db_path or SIDECAR_DB_PATH
    if not path.exists():
        raise FileNotFoundError(
            f"Sidecar database not found at {path}. "
            "Run qbit at least once to initialize the database."
        )
    return lancedb.connect(str(path))


def get_last_session(db: lancedb.DBConnection) -> Optional[dict]:
    """
    Get the most recent session by started_at_ms timestamp.

    Args:
        db: LanceDB connection

    Returns:
        Dictionary containing session data, or None if no sessions exist
    """
    try:
        sessions_table = db.open_table("sessions")
        df = sessions_table.to_pandas()
        if df.empty:
            return None
        # Sort by started_at_ms descending and get first row
        df_sorted = df.sort_values("started_at_ms", ascending=False)
        return df_sorted.iloc[0].to_dict()
    except Exception:
        # Table doesn't exist or other error
        return None


def get_session(db: lancedb.DBConnection, session_id: str) -> Optional[dict]:
    """
    Get a specific session by ID.

    Args:
        db: LanceDB connection
        session_id: Session identifier

    Returns:
        Dictionary containing session data, or None if not found
    """
    try:
        sessions_table = db.open_table("sessions")
        df = sessions_table.to_pandas()
        if df.empty:
            return None
        # Filter by session ID
        session_df = df[df["id"] == session_id]
        if session_df.empty:
            return None
        return session_df.iloc[0].to_dict()
    except Exception:
        return None


def get_session_events(db: lancedb.DBConnection, session_id: str) -> list[dict]:
    """
    Get all events for a specific session, sorted by timestamp.

    Args:
        db: LanceDB connection
        session_id: Session identifier

    Returns:
        List of event dictionaries, sorted by timestamp_ms (ascending)
    """
    try:
        events_table = db.open_table("events")
        df = events_table.to_pandas()
        if df.empty:
            return []
        # Filter by session_id and sort by timestamp
        session_events = df[df["session_id"] == session_id]
        session_events_sorted = session_events.sort_values("timestamp_ms")
        return session_events_sorted.to_dict("records")
    except Exception:
        return []


def search_events_keyword(
    db: lancedb.DBConnection, query: str, limit: int = 10
) -> list[dict]:
    """
    Search events by keyword in content field.

    Args:
        db: LanceDB connection
        query: Search keyword or phrase
        limit: Maximum number of results to return

    Returns:
        List of event dictionaries matching the query, most recent first
    """
    try:
        events_table = db.open_table("events")
        df = events_table.to_pandas()
        if df.empty:
            return []
        # Filter events where content contains the query (case-insensitive)
        if "content" not in df.columns:
            return []
        # Handle None values in content column
        df_filtered = df[
            df["content"].fillna("").str.contains(query, case=False, na=False)
        ]
        # Sort by timestamp descending and limit
        df_sorted = df_filtered.sort_values("timestamp_ms", ascending=False)
        return df_sorted.head(limit).to_dict("records")
    except Exception:
        return []


def get_storage_stats(db: lancedb.DBConnection) -> dict:
    """
    Get statistics about the sidecar database storage.

    Args:
        db: LanceDB connection

    Returns:
        Dictionary with keys: event_count, checkpoint_count, session_count
    """
    stats = {"event_count": 0, "checkpoint_count": 0, "session_count": 0}

    try:
        events_table = db.open_table("events")
        stats["event_count"] = len(events_table.to_pandas())
    except Exception:
        pass

    try:
        checkpoints_table = db.open_table("checkpoints")
        stats["checkpoint_count"] = len(checkpoints_table.to_pandas())
    except Exception:
        pass

    try:
        sessions_table = db.open_table("sessions")
        stats["session_count"] = len(sessions_table.to_pandas())
    except Exception:
        pass

    return stats


def list_sessions(db: lancedb.DBConnection, limit: int = 10) -> list[dict]:
    """
    List recent sessions, most recent first.

    Args:
        db: LanceDB connection
        limit: Maximum number of sessions to return

    Returns:
        List of session dictionaries, sorted by started_at_ms (descending)
    """
    try:
        sessions_table = db.open_table("sessions")
        df = sessions_table.to_pandas()
        if df.empty:
            return []
        # Sort by started_at_ms descending and limit
        df_sorted = df.sort_values("started_at_ms", ascending=False)
        return df_sorted.head(limit).to_dict("records")
    except Exception:
        return []
