"""Qbit Sidecar database utilities.

The sidecar is a background service that captures agent interactions into
a LanceDB vector database. This package provides utilities for querying
that database in evaluation tests.

Quick Start:
    >>> from sidecar import connect_db, get_sessions, get_events
    >>>
    >>> db = connect_db()
    >>> sessions = get_sessions(db)
    >>> events = get_events(db, sessions[0]["id"])

Database Location:
    ~/.qbit/sidecar/sidecar.lance

Schema Overview:
    Legacy Tables:
        - events: Raw terminal and AI events with embeddings
        - sessions: Session metadata (deprecated, use l1_sessions)

    Layer 1 Normalized Tables:
        - l1_sessions: Session metadata
        - l1_goals: Goal stack with hierarchy
        - l1_decisions: Decision log with rationale
        - l1_errors: Error tracking
        - l1_file_contexts: File understanding
        - l1_questions: Open questions
        - l1_goal_progress: Goal progress notes
        - l1_file_changes: File modification history
"""

from .db import (
    # Connection
    connect_sidecar_db as connect_db,
    # Session queries
    get_last_session,
    get_session,
    list_sessions as get_sessions,
    # Event queries
    get_session_events as get_events,
    search_events_keyword as search_events,
    # Layer 1 queries
    check_l1_tables_exist,
    get_l1_sessions,
    get_l1_goals,
    get_l1_decisions,
    get_l1_errors,
    get_l1_file_contexts,
    get_l1_questions,
    get_l1_decisions_by_category,
    get_l1_unresolved_errors,
    get_l1_table_stats,
    # Stats
    get_storage_stats,
)

__all__ = [
    # Connection
    "connect_db",
    # Session queries
    "get_last_session",
    "get_session",
    "get_sessions",
    # Event queries
    "get_events",
    "search_events",
    # Layer 1 queries
    "check_l1_tables_exist",
    "get_l1_sessions",
    "get_l1_goals",
    "get_l1_decisions",
    "get_l1_errors",
    "get_l1_file_contexts",
    "get_l1_questions",
    "get_l1_decisions_by_category",
    "get_l1_unresolved_errors",
    "get_l1_table_stats",
    # Stats
    "get_storage_stats",
]
