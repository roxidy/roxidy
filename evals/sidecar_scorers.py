"""
Custom scorers for verifying sidecar state in qbit evaluations.

This module provides factory functions that create scorer callables for validating
session data stored in the qbit sidecar LanceDB database. Each factory returns a
scorer function that takes a session_id and returns (passed: bool, reason: str).

Example usage:
    >>> from sidecar_scorers import verify_min_event_count, verify_files_tracked
    >>> from sidecar_utils import connect_sidecar_db, get_last_session
    >>>
    >>> # Create scorers
    >>> check_events = verify_min_event_count(5)
    >>> check_files = verify_files_tracked(["main.rs", "lib.rs"])
    >>>
    >>> # Get session and score
    >>> db = connect_sidecar_db()
    >>> session = get_last_session(db)
    >>> passed, reason = check_events(session["id"])
    >>> print(f"Event count check: {passed} - {reason}")
"""

import json
from typing import Callable, Optional

from sidecar_utils import connect_sidecar_db, get_session, get_session_events


# Type alias for scorer functions
Scorer = Callable[[str], tuple[bool, str]]


def verify_min_event_count(expected_min: int) -> Scorer:
    """
    Create a scorer that verifies a session has at least N events.

    Args:
        expected_min: Minimum number of events required

    Returns:
        Scorer function that takes session_id and returns (passed, reason)

    Example:
        >>> scorer = verify_min_event_count(10)
        >>> passed, reason = scorer("session-abc-123")
        >>> # Returns: (True, "Session has 15 events (expected ≥10)")
        >>> # Or: (False, "Session has 5 events (expected ≥10)")
    """

    def scorer(session_id: str) -> tuple[bool, str]:
        try:
            db = connect_sidecar_db()
            events = get_session_events(db, session_id)
            event_count = len(events)

            if event_count >= expected_min:
                return (
                    True,
                    f"Session has {event_count} events (expected ≥{expected_min})",
                )
            else:
                return (
                    False,
                    f"Session has {event_count} events (expected ≥{expected_min})",
                )
        except Exception as e:
            return (False, f"Error checking event count: {e}")

    return scorer


def verify_files_tracked(expected_files: list[str]) -> Scorer:
    """
    Create a scorer that verifies specific files appear in session's files_touched.

    The files_touched field may be stored as either a JSON string or a list.
    File paths are matched by checking if the expected filename appears anywhere
    in the full path (e.g., "main.rs" matches "/path/to/main.rs").

    Args:
        expected_files: List of filenames to check for

    Returns:
        Scorer function that takes session_id and returns (passed, reason)

    Example:
        >>> scorer = verify_files_tracked(["main.rs", "lib.rs"])
        >>> passed, reason = scorer("session-abc-123")
        >>> # Returns: (True, "All expected files found: main.rs, lib.rs")
        >>> # Or: (False, "Missing files: lib.rs")
    """

    def scorer(session_id: str) -> tuple[bool, str]:
        try:
            db = connect_sidecar_db()
            session = get_session(db, session_id)

            if not session:
                return (False, f"Session {session_id} not found")

            files_touched = session.get("files_touched_json")
            if files_touched is None:
                return (False, "Session has no files_touched_json field")

            # Parse files_touched if it's a JSON string
            if isinstance(files_touched, str):
                try:
                    files_list = json.loads(files_touched)
                except json.JSONDecodeError:
                    return (False, "Invalid JSON in files_touched_json")
            elif isinstance(files_touched, list):
                files_list = files_touched
            else:
                return (
                    False,
                    f"Unexpected type for files_touched_json: {type(files_touched)}",
                )

            # Check each expected file
            missing_files = []
            for expected_file in expected_files:
                # Check if the filename appears in any of the tracked file paths
                found = any(expected_file in path for path in files_list)
                if not found:
                    missing_files.append(expected_file)

            if not missing_files:
                return (
                    True,
                    f"All expected files found: {', '.join(expected_files)}",
                )
            else:
                return (False, f"Missing files: {', '.join(missing_files)}")

        except Exception as e:
            return (False, f"Error checking files_tracked: {e}")

    return scorer


def verify_event_types_present(expected_types: list[str]) -> Scorer:
    """
    Create a scorer that verifies specific event types are present in the session.

    Event types are checked against the event_type field of session events.

    Args:
        expected_types: List of event types to check for (e.g., ["UserPrompt", "ToolCall"])

    Returns:
        Scorer function that takes session_id and returns (passed, reason)

    Example:
        >>> scorer = verify_event_types_present(["UserPrompt", "ToolCall", "AiResponse"])
        >>> passed, reason = scorer("session-abc-123")
        >>> # Returns: (True, "All expected event types found: UserPrompt, ToolCall, AiResponse")
        >>> # Or: (False, "Missing event types: AiResponse")
    """

    def scorer(session_id: str) -> tuple[bool, str]:
        try:
            db = connect_sidecar_db()
            events = get_session_events(db, session_id)

            if not events:
                return (False, "Session has no events")

            # Collect all event types present
            event_types_found = {event.get("event_type") for event in events}

            # Check for missing types
            missing_types = [
                et for et in expected_types if et not in event_types_found
            ]

            if not missing_types:
                return (
                    True,
                    f"All expected event types found: {', '.join(expected_types)}",
                )
            else:
                return (False, f"Missing event types: {', '.join(missing_types)}")

        except Exception as e:
            return (False, f"Error checking event types: {e}")

    return scorer


def verify_session_ended() -> Scorer:
    """
    Create a scorer that verifies a session has an ended_at_ms timestamp.

    This checks if the session was properly closed.

    Returns:
        Scorer function that takes session_id and returns (passed, reason)

    Example:
        >>> scorer = verify_session_ended()
        >>> passed, reason = scorer("session-abc-123")
        >>> # Returns: (True, "Session ended at timestamp 1701234567890")
        >>> # Or: (False, "Session has not ended (ended_at_ms is None)")
    """

    def scorer(session_id: str) -> tuple[bool, str]:
        try:
            db = connect_sidecar_db()
            session = get_session(db, session_id)

            if not session:
                return (False, f"Session {session_id} not found")

            ended_at_ms = session.get("ended_at_ms")

            if ended_at_ms is not None and ended_at_ms > 0:
                return (True, f"Session ended at timestamp {ended_at_ms}")
            else:
                return (False, "Session has not ended (ended_at_ms is None or 0)")

        except Exception as e:
            return (False, f"Error checking session end status: {e}")

    return scorer


def verify_event_content_contains(keyword: str) -> Scorer:
    """
    Create a scorer that verifies at least one event contains a specific keyword.

    Search is case-insensitive and checks the content field of events.

    Args:
        keyword: Keyword to search for in event content

    Returns:
        Scorer function that takes session_id and returns (passed, reason)

    Example:
        >>> scorer = verify_event_content_contains("git commit")
        >>> passed, reason = scorer("session-abc-123")
        >>> # Returns: (True, "Keyword 'git commit' found in event #3")
        >>> # Or: (False, "Keyword 'git commit' not found in any event")
    """

    def scorer(session_id: str) -> tuple[bool, str]:
        try:
            db = connect_sidecar_db()
            events = get_session_events(db, session_id)

            if not events:
                return (False, "Session has no events")

            keyword_lower = keyword.lower()

            # Search through events
            for idx, event in enumerate(events, start=1):
                content = event.get("content", "")
                if content and keyword_lower in content.lower():
                    return (True, f"Keyword '{keyword}' found in event #{idx}")

            return (False, f"Keyword '{keyword}' not found in any event")

        except Exception as e:
            return (False, f"Error searching event content: {e}")

    return scorer


def verify_event_sequence(expected_sequence: list[str]) -> Scorer:
    """
    Create a scorer that verifies event types appear in the specified relative order.

    The events don't have to be consecutive, but they must appear in the given
    order relative to each other.

    Args:
        expected_sequence: List of event types in expected order

    Returns:
        Scorer function that takes session_id and returns (passed, reason)

    Example:
        >>> scorer = verify_event_sequence(["UserPrompt", "ToolCall", "AiResponse"])
        >>> passed, reason = scorer("session-abc-123")
        >>> # Returns: (True, "Event sequence verified: UserPrompt → ToolCall → AiResponse")
        >>> # Or: (False, "Event sequence broken: found ToolCall at index 5 before UserPrompt at index 10")
    """

    def scorer(session_id: str) -> tuple[bool, str]:
        try:
            db = connect_sidecar_db()
            events = get_session_events(db, session_id)

            if not events:
                return (False, "Session has no events")

            # Find the first occurrence of each expected event type
            event_indices: dict[str, Optional[int]] = {et: None for et in expected_sequence}

            for idx, event in enumerate(events):
                event_type = event.get("event_type")
                if event_type in event_indices and event_indices[event_type] is None:
                    event_indices[event_type] = idx

            # Check if all expected types were found
            missing_types = [et for et, idx in event_indices.items() if idx is None]
            if missing_types:
                return (
                    False,
                    f"Event sequence incomplete: missing {', '.join(missing_types)}",
                )

            # Check if indices are in ascending order
            indices = [event_indices[et] for et in expected_sequence]
            for i in range(len(indices) - 1):
                if indices[i] >= indices[i + 1]:
                    return (
                        False,
                        f"Event sequence broken: found {expected_sequence[i+1]} at index {indices[i+1]} "
                        f"before {expected_sequence[i]} at index {indices[i]}",
                    )

            sequence_str = " → ".join(expected_sequence)
            return (True, f"Event sequence verified: {sequence_str}")

        except Exception as e:
            return (False, f"Error checking event sequence: {e}")

    return scorer


# =============================================================================
# Layer 1 Session State Scorers
# =============================================================================

from sidecar_utils import (
    get_layer1_state,
    get_layer1_goals,
    get_layer1_decisions,
    get_layer1_file_contexts,
    get_layer1_errors,
    get_layer1_open_questions,
    get_layer1_narrative,
    get_layer1_state_count,
)


def verify_layer1_has_goal() -> Scorer:
    """
    Create a scorer that verifies the Layer 1 state has at least one goal.

    Returns:
        Scorer function that takes session_id and returns (passed, reason)

    Example:
        >>> scorer = verify_layer1_has_goal()
        >>> passed, reason = scorer("session-abc-123")
        >>> # Returns: (True, "Layer 1 has 2 goal(s): 'Implement user auth', 'Add tests'")
        >>> # Or: (False, "Layer 1 state has no goals")
    """

    def scorer(session_id: str) -> tuple[bool, str]:
        try:
            db = connect_sidecar_db()
            state = get_layer1_state(db, session_id)

            if state is None:
                return (False, f"No Layer 1 state found for session {session_id}")

            goals = get_layer1_goals(state)
            if not goals:
                return (False, "Layer 1 state has no goals")

            goal_descriptions = [g.get("description", "?")[:50] for g in goals]
            return (
                True,
                f"Layer 1 has {len(goals)} goal(s): {', '.join(repr(d) for d in goal_descriptions)}",
            )

        except Exception as e:
            return (False, f"Error checking Layer 1 goals: {e}")

    return scorer


def verify_layer1_goal_contains(keyword: str) -> Scorer:
    """
    Create a scorer that verifies at least one goal contains a specific keyword.

    Args:
        keyword: Keyword to search for in goal descriptions (case-insensitive)

    Returns:
        Scorer function that takes session_id and returns (passed, reason)

    Example:
        >>> scorer = verify_layer1_goal_contains("authentication")
        >>> passed, reason = scorer("session-abc-123")
        >>> # Returns: (True, "Goal #1 contains 'authentication': 'Implement user authentication'")
    """

    def scorer(session_id: str) -> tuple[bool, str]:
        try:
            db = connect_sidecar_db()
            state = get_layer1_state(db, session_id)

            if state is None:
                return (False, f"No Layer 1 state found for session {session_id}")

            goals = get_layer1_goals(state)
            keyword_lower = keyword.lower()

            for idx, goal in enumerate(goals, start=1):
                description = goal.get("description", "")
                if keyword_lower in description.lower():
                    return (
                        True,
                        f"Goal #{idx} contains '{keyword}': '{description[:60]}'",
                    )

            return (False, f"No goal contains keyword '{keyword}'")

        except Exception as e:
            return (False, f"Error checking Layer 1 goal content: {e}")

    return scorer


def verify_layer1_has_decisions(min_count: int = 1) -> Scorer:
    """
    Create a scorer that verifies the Layer 1 state has at least N decisions.

    Args:
        min_count: Minimum number of decisions required (default: 1)

    Returns:
        Scorer function that takes session_id and returns (passed, reason)

    Example:
        >>> scorer = verify_layer1_has_decisions(2)
        >>> passed, reason = scorer("session-abc-123")
        >>> # Returns: (True, "Layer 1 has 3 decision(s) (expected ≥2)")
    """

    def scorer(session_id: str) -> tuple[bool, str]:
        try:
            db = connect_sidecar_db()
            state = get_layer1_state(db, session_id)

            if state is None:
                return (False, f"No Layer 1 state found for session {session_id}")

            decisions = get_layer1_decisions(state)
            count = len(decisions)

            if count >= min_count:
                return (True, f"Layer 1 has {count} decision(s) (expected ≥{min_count})")
            else:
                return (False, f"Layer 1 has {count} decision(s) (expected ≥{min_count})")

        except Exception as e:
            return (False, f"Error checking Layer 1 decisions: {e}")

    return scorer


def verify_layer1_decision_contains(keyword: str) -> Scorer:
    """
    Create a scorer that verifies at least one decision contains a specific keyword.

    Searches in both 'choice' and 'rationale' fields (case-insensitive).

    Args:
        keyword: Keyword to search for in decisions

    Returns:
        Scorer function that takes session_id and returns (passed, reason)
    """

    def scorer(session_id: str) -> tuple[bool, str]:
        try:
            db = connect_sidecar_db()
            state = get_layer1_state(db, session_id)

            if state is None:
                return (False, f"No Layer 1 state found for session {session_id}")

            decisions = get_layer1_decisions(state)
            keyword_lower = keyword.lower()

            for idx, decision in enumerate(decisions, start=1):
                choice = decision.get("choice", "")
                rationale = decision.get("rationale", "")
                combined = f"{choice} {rationale}"

                if keyword_lower in combined.lower():
                    return (
                        True,
                        f"Decision #{idx} contains '{keyword}': choice='{choice[:40]}'",
                    )

            return (False, f"No decision contains keyword '{keyword}'")

        except Exception as e:
            return (False, f"Error checking Layer 1 decision content: {e}")

    return scorer


def verify_layer1_has_file_context(file_pattern: str) -> Scorer:
    """
    Create a scorer that verifies a specific file is tracked in file_contexts.

    Args:
        file_pattern: Filename or pattern to search for in tracked file paths

    Returns:
        Scorer function that takes session_id and returns (passed, reason)

    Example:
        >>> scorer = verify_layer1_has_file_context("main.rs")
        >>> passed, reason = scorer("session-abc-123")
        >>> # Returns: (True, "File 'main.rs' tracked: '/path/to/main.rs'")
    """

    def scorer(session_id: str) -> tuple[bool, str]:
        try:
            db = connect_sidecar_db()
            state = get_layer1_state(db, session_id)

            if state is None:
                return (False, f"No Layer 1 state found for session {session_id}")

            file_contexts = get_layer1_file_contexts(state)
            if not file_contexts:
                return (False, "Layer 1 state has no file contexts")

            # Search for the file pattern in tracked paths
            for path in file_contexts.keys():
                if file_pattern in str(path):
                    return (True, f"File '{file_pattern}' tracked: '{path}'")

            tracked = list(file_contexts.keys())[:5]
            return (
                False,
                f"File '{file_pattern}' not found. Tracked: {tracked}",
            )

        except Exception as e:
            return (False, f"Error checking Layer 1 file context: {e}")

    return scorer


def verify_layer1_file_count(min_count: int = 1) -> Scorer:
    """
    Create a scorer that verifies at least N files are tracked.

    Args:
        min_count: Minimum number of tracked files required

    Returns:
        Scorer function that takes session_id and returns (passed, reason)
    """

    def scorer(session_id: str) -> tuple[bool, str]:
        try:
            db = connect_sidecar_db()
            state = get_layer1_state(db, session_id)

            if state is None:
                return (False, f"No Layer 1 state found for session {session_id}")

            file_contexts = get_layer1_file_contexts(state)
            count = len(file_contexts)

            if count >= min_count:
                return (True, f"Layer 1 tracks {count} file(s) (expected ≥{min_count})")
            else:
                return (False, f"Layer 1 tracks {count} file(s) (expected ≥{min_count})")

        except Exception as e:
            return (False, f"Error checking Layer 1 file count: {e}")

    return scorer


def verify_layer1_has_narrative() -> Scorer:
    """
    Create a scorer that verifies the Layer 1 state has a non-empty narrative.

    Returns:
        Scorer function that takes session_id and returns (passed, reason)

    Example:
        >>> scorer = verify_layer1_has_narrative()
        >>> passed, reason = scorer("session-abc-123")
        >>> # Returns: (True, "Layer 1 has narrative (142 chars): 'Working on...'")
    """

    def scorer(session_id: str) -> tuple[bool, str]:
        try:
            db = connect_sidecar_db()
            state = get_layer1_state(db, session_id)

            if state is None:
                return (False, f"No Layer 1 state found for session {session_id}")

            narrative = get_layer1_narrative(state)
            if narrative and narrative.strip():
                preview = narrative[:60] + "..." if len(narrative) > 60 else narrative
                return (
                    True,
                    f"Layer 1 has narrative ({len(narrative)} chars): '{preview}'",
                )
            else:
                return (False, "Layer 1 state has no narrative")

        except Exception as e:
            return (False, f"Error checking Layer 1 narrative: {e}")

    return scorer


def verify_layer1_narrative_contains(keyword: str) -> Scorer:
    """
    Create a scorer that verifies the narrative contains a specific keyword.

    Args:
        keyword: Keyword to search for in the narrative (case-insensitive)

    Returns:
        Scorer function that takes session_id and returns (passed, reason)
    """

    def scorer(session_id: str) -> tuple[bool, str]:
        try:
            db = connect_sidecar_db()
            state = get_layer1_state(db, session_id)

            if state is None:
                return (False, f"No Layer 1 state found for session {session_id}")

            narrative = get_layer1_narrative(state)
            if not narrative:
                return (False, "Layer 1 state has no narrative")

            if keyword.lower() in narrative.lower():
                return (True, f"Narrative contains '{keyword}'")
            else:
                return (False, f"Narrative does not contain '{keyword}'")

        except Exception as e:
            return (False, f"Error checking Layer 1 narrative content: {e}")

    return scorer


def verify_layer1_has_errors(min_count: int = 1) -> Scorer:
    """
    Create a scorer that verifies at least N errors are recorded.

    Args:
        min_count: Minimum number of errors required

    Returns:
        Scorer function that takes session_id and returns (passed, reason)
    """

    def scorer(session_id: str) -> tuple[bool, str]:
        try:
            db = connect_sidecar_db()
            state = get_layer1_state(db, session_id)

            if state is None:
                return (False, f"No Layer 1 state found for session {session_id}")

            errors = get_layer1_errors(state)
            count = len(errors)

            if count >= min_count:
                return (True, f"Layer 1 has {count} error(s) (expected ≥{min_count})")
            else:
                return (False, f"Layer 1 has {count} error(s) (expected ≥{min_count})")

        except Exception as e:
            return (False, f"Error checking Layer 1 errors: {e}")

    return scorer


def verify_layer1_has_open_questions(min_count: int = 1) -> Scorer:
    """
    Create a scorer that verifies at least N open questions are recorded.

    Args:
        min_count: Minimum number of open questions required

    Returns:
        Scorer function that takes session_id and returns (passed, reason)
    """

    def scorer(session_id: str) -> tuple[bool, str]:
        try:
            db = connect_sidecar_db()
            state = get_layer1_state(db, session_id)

            if state is None:
                return (False, f"No Layer 1 state found for session {session_id}")

            questions = get_layer1_open_questions(state)
            count = len(questions)

            if count >= min_count:
                return (True, f"Layer 1 has {count} open question(s) (expected ≥{min_count})")
            else:
                return (False, f"Layer 1 has {count} open question(s) (expected ≥{min_count})")

        except Exception as e:
            return (False, f"Error checking Layer 1 open questions: {e}")

    return scorer


def verify_layer1_question_contains(keyword: str) -> Scorer:
    """
    Create a scorer that verifies at least one open question contains a keyword.

    Args:
        keyword: Keyword to search for in questions (case-insensitive)

    Returns:
        Scorer function that takes session_id and returns (passed, reason)
    """

    def scorer(session_id: str) -> tuple[bool, str]:
        try:
            db = connect_sidecar_db()
            state = get_layer1_state(db, session_id)

            if state is None:
                return (False, f"No Layer 1 state found for session {session_id}")

            questions = get_layer1_open_questions(state)
            keyword_lower = keyword.lower()

            for idx, q in enumerate(questions, start=1):
                question_text = q.get("question", "")
                if keyword_lower in question_text.lower():
                    return (
                        True,
                        f"Question #{idx} contains '{keyword}': '{question_text[:50]}'",
                    )

            return (False, f"No open question contains keyword '{keyword}'")

        except Exception as e:
            return (False, f"Error checking Layer 1 question content: {e}")

    return scorer


def verify_layer1_state_exists() -> Scorer:
    """
    Create a scorer that verifies a Layer 1 state exists for the session.

    Returns:
        Scorer function that takes session_id and returns (passed, reason)
    """

    def scorer(session_id: str) -> tuple[bool, str]:
        try:
            db = connect_sidecar_db()
            state = get_layer1_state(db, session_id)

            if state is not None:
                return (True, f"Layer 1 state exists for session {session_id}")
            else:
                return (False, f"No Layer 1 state found for session {session_id}")

        except Exception as e:
            return (False, f"Error checking Layer 1 state existence: {e}")

    return scorer


def verify_layer1_snapshots(min_count: int = 1) -> Scorer:
    """
    Create a scorer that verifies at least N state snapshots exist for the session.

    Args:
        min_count: Minimum number of snapshots required

    Returns:
        Scorer function that takes session_id and returns (passed, reason)
    """

    def scorer(session_id: str) -> tuple[bool, str]:
        try:
            db = connect_sidecar_db()
            count = get_layer1_state_count(db, session_id)

            if count >= min_count:
                return (True, f"Layer 1 has {count} snapshot(s) (expected ≥{min_count})")
            else:
                return (False, f"Layer 1 has {count} snapshot(s) (expected ≥{min_count})")

        except Exception as e:
            return (False, f"Error checking Layer 1 snapshot count: {e}")

    return scorer
