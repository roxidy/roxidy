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


# =============================================================================
# Layer 1 Session State Utilities
# =============================================================================


def get_layer1_state(db: lancedb.DBConnection, session_id: str) -> Optional[dict]:
    """
    Get the latest Layer 1 session state for a specific session.

    The Layer 1 state includes goals, narrative, decisions, file contexts,
    errors, and open questions - a continuous model of the session.

    Args:
        db: LanceDB connection
        session_id: Session identifier

    Returns:
        Dictionary containing the parsed state_json, or None if not found
    """
    try:
        states_table = db.open_table("session_states")
        df = states_table.to_pandas()
        if df.empty:
            return None

        # Filter by session_id and get latest by timestamp
        session_states = df[df["session_id"] == session_id]
        if session_states.empty:
            return None

        # Sort by timestamp descending and get latest
        latest = session_states.sort_values("timestamp_ms", ascending=False).iloc[0]

        # Parse state_json
        state_json = latest.get("state_json")
        if state_json is None:
            return None

        if isinstance(state_json, str):
            import json
            return json.loads(state_json)
        return state_json
    except Exception:
        return None


def get_layer1_latest(db: lancedb.DBConnection) -> Optional[dict]:
    """
    Get the most recent Layer 1 session state across all sessions.

    Args:
        db: LanceDB connection

    Returns:
        Dictionary containing the parsed state_json, or None if not found
    """
    try:
        states_table = db.open_table("session_states")
        df = states_table.to_pandas()
        if df.empty:
            return None

        # Get latest by timestamp across all sessions
        latest = df.sort_values("timestamp_ms", ascending=False).iloc[0]

        # Parse state_json
        state_json = latest.get("state_json")
        if state_json is None:
            return None

        if isinstance(state_json, str):
            import json
            return json.loads(state_json)
        return state_json
    except Exception:
        return None


def get_layer1_goals(state: dict) -> list[dict]:
    """
    Extract goals from a Layer 1 state.

    Args:
        state: Parsed Layer 1 state dictionary

    Returns:
        List of goal dictionaries from the goal_stack
    """
    return state.get("goal_stack", [])


def get_layer1_decisions(state: dict) -> list[dict]:
    """
    Extract decisions from a Layer 1 state.

    Args:
        state: Parsed Layer 1 state dictionary

    Returns:
        List of decision dictionaries
    """
    return state.get("decisions", [])


def get_layer1_file_contexts(state: dict) -> dict:
    """
    Extract file contexts from a Layer 1 state.

    Args:
        state: Parsed Layer 1 state dictionary

    Returns:
        Dictionary mapping file paths to file context info
    """
    return state.get("file_contexts", {})


def get_layer1_errors(state: dict) -> list[dict]:
    """
    Extract errors from a Layer 1 state.

    Args:
        state: Parsed Layer 1 state dictionary

    Returns:
        List of error entry dictionaries
    """
    return state.get("errors", [])


def get_layer1_open_questions(state: dict) -> list[dict]:
    """
    Extract open questions from a Layer 1 state.

    Args:
        state: Parsed Layer 1 state dictionary

    Returns:
        List of open question dictionaries
    """
    return state.get("open_questions", [])


def get_layer1_narrative(state: dict) -> str:
    """
    Extract narrative from a Layer 1 state.

    Args:
        state: Parsed Layer 1 state dictionary

    Returns:
        The narrative string, or empty string if not present
    """
    return state.get("narrative", "")


def list_layer1_states(db: lancedb.DBConnection, limit: int = 10) -> list[dict]:
    """
    List recent Layer 1 session states, most recent first.

    Args:
        db: LanceDB connection
        limit: Maximum number of states to return

    Returns:
        List of state metadata (session_id, timestamp_ms), without full state_json
    """
    try:
        states_table = db.open_table("session_states")
        df = states_table.to_pandas()
        if df.empty:
            return []

        # Sort by timestamp descending and select columns
        df_sorted = df.sort_values("timestamp_ms", ascending=False)
        # Return metadata only, not the full state_json
        cols = ["session_id", "timestamp_ms"]
        available_cols = [c for c in cols if c in df_sorted.columns]
        return df_sorted[available_cols].head(limit).to_dict("records")
    except Exception:
        return []


def get_layer1_state_count(db: lancedb.DBConnection, session_id: Optional[str] = None) -> int:
    """
    Get the number of Layer 1 state snapshots.

    Args:
        db: LanceDB connection
        session_id: Optional session identifier. If None, counts all snapshots.

    Returns:
        Number of state snapshots stored
    """
    try:
        states_table = db.open_table("session_states")
        df = states_table.to_pandas()
        if df.empty:
            return 0
        if session_id:
            return len(df[df["session_id"] == session_id])
        return len(df)
    except Exception:
        return 0


def get_layer1_injectable_context(state: dict, max_length: int = 2000) -> str:
    """
    Generate injectable context string from Layer 1 state.

    This creates a concise summary suitable for injection into agent prompts,
    containing goals, recent decisions, and narrative.

    Args:
        state: Parsed Layer 1 state dictionary
        max_length: Maximum length of returned context

    Returns:
        Formatted context string
    """
    parts = []

    # Goals
    goals = get_layer1_goals(state)
    if goals:
        goal_lines = []
        for g in goals[:3]:  # Top 3 goals
            desc = g.get('description', '')[:100]
            priority = g.get('priority', '')
            completed = g.get('completed', False)
            status = '✓' if completed else '○'
            goal_lines.append(f"  {status} [{priority}] {desc}")
        parts.append("GOALS:\n" + "\n".join(goal_lines))

    # Narrative
    narrative = get_layer1_narrative(state)
    if narrative:
        parts.append(f"NARRATIVE:\n  {narrative[:300]}")

    # Recent decisions
    decisions = get_layer1_decisions(state)
    if decisions:
        decision_lines = []
        for d in decisions[:2]:  # Last 2 decisions
            choice = d.get('choice', '')[:80]
            rationale = d.get('rationale', '')[:50]
            decision_lines.append(f"  - {choice}")
            if rationale:
                decision_lines.append(f"    Reason: {rationale}")
        parts.append("RECENT DECISIONS:\n" + "\n".join(decision_lines))

    # File context summary
    file_contexts = get_layer1_file_contexts(state)
    if file_contexts:
        file_lines = []
        for path, ctx in list(file_contexts.items())[:5]:
            summary = ctx.get('summary', '')[:60]
            file_lines.append(f"  - {path}: {summary}")
        parts.append("FILE CONTEXT:\n" + "\n".join(file_lines))

    # Errors
    errors = get_layer1_errors(state)
    if errors:
        error_lines = []
        for e in errors[:2]:
            msg = e.get('message', '')[:60]
            resolved = e.get('resolved', False)
            error_lines.append(f"  - {'[RESOLVED]' if resolved else '[OPEN]'} {msg}")
        parts.append("ERRORS:\n" + "\n".join(error_lines))

    # Open questions
    questions = get_layer1_open_questions(state)
    if questions:
        q_lines = []
        for q in questions[:2]:
            question = q.get('question', '')[:60]
            priority = q.get('priority', 'unknown')
            q_lines.append(f"  - [{priority}] {question}")
        parts.append("OPEN QUESTIONS:\n" + "\n".join(q_lines))

    context = "\n\n".join(parts)

    # Truncate if needed
    if len(context) > max_length:
        context = context[:max_length - 3] + "..."

    return context
