"""Evaluation Configuration.

Centralized configuration management for the eval framework.

Environment Variables:
    RUN_API_TESTS: Enable tests that require API credentials (1, true, yes)
    QBIT_EVAL_MODEL: Override agent model for evaluations
    QBIT_CLI_PATH: Path to qbit-cli binary
    OPENAI_API_KEY: API key for DeepEval evaluator
    VERBOSE: Enable verbose test output (1, true, yes)

Settings File:
    ~/.qbit/settings.toml

    [eval]
    model = "gpt-4o-mini"           # DeepEval evaluator model
    agent_model = "claude-..."      # Qbit agent model
    api_key = "sk-..."              # OpenAI API key (optional)

Environment files loaded (in order):
    1. ../.env (project root)
    2. .env (evals directory)
"""

import os
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from dotenv import load_dotenv

# Load .env files (parent directory first, then local)
_project_root = Path(__file__).parent.parent
load_dotenv(_project_root / ".env")
load_dotenv()  # Also load local .env if present

# =============================================================================
# Timeout Constants
# =============================================================================

TIMEOUT_DEFAULT = 60   # 1 minute for single operations
TIMEOUT_BATCH = 180    # 3 minutes for batch operations

# =============================================================================
# Paths
# =============================================================================

PROJECT_ROOT = Path(__file__).parent.parent
SETTINGS_PATH = Path.home() / ".qbit" / "settings.toml"
DEFAULT_BINARY_PATH = PROJECT_ROOT / "src-tauri" / "target" / "debug" / "qbit-cli"


# =============================================================================
# Settings Loading
# =============================================================================


def load_settings() -> dict[str, Any]:
    """Load settings from ~/.qbit/settings.toml.

    Returns:
        Settings dict, or defaults if file doesn't exist.
    """
    if not SETTINGS_PATH.exists():
        return {"eval": {"model": "gpt-4o-mini", "temperature": 0}}
    with open(SETTINGS_PATH, "rb") as f:
        return tomllib.load(f)


def get_binary_path() -> Path:
    """Get the path to the qbit-cli binary.

    Priority:
        1. QBIT_CLI_PATH environment variable
        2. Default debug build location

    Returns:
        Path to the qbit-cli binary.
    """
    if path := os.environ.get("QBIT_CLI_PATH"):
        return Path(path)
    return DEFAULT_BINARY_PATH


def get_eval_model_name() -> str:
    """Get the DeepEval evaluator model name.

    Priority:
        1. settings.toml [eval].model
        2. Default: gpt-4o-mini

    Returns:
        Model name string.
    """
    settings = load_settings()
    return settings.get("eval", {}).get("model", "gpt-4o-mini")


def get_agent_model() -> str | None:
    """Get the Qbit agent model for evaluations.

    Priority:
        1. QBIT_EVAL_MODEL environment variable
        2. settings.toml [eval].agent_model
        3. None (use server default)

    Returns:
        Model name string or None.
    """
    if model := os.environ.get("QBIT_EVAL_MODEL"):
        return model
    settings = load_settings()
    return settings.get("eval", {}).get("agent_model")


def is_verbose() -> bool:
    """Check if verbose mode is enabled.

    Returns:
        True if VERBOSE env var is set to 1/true/yes.
    """
    return os.environ.get("VERBOSE", "").lower() in ("1", "true", "yes")


def is_api_tests_enabled() -> bool:
    """Check if API tests should run.

    Returns:
        True if RUN_API_TESTS env var is set to 1/true/yes.
    """
    return os.environ.get("RUN_API_TESTS", "").lower() in ("1", "true", "yes")


# =============================================================================
# Eval Model Factory
# =============================================================================


def create_eval_model():
    """Create the DeepEval evaluator model.

    Loads configuration from settings.toml and environment.
    Sets OPENAI_API_KEY if provided in settings.

    Returns:
        GPTModel instance for DeepEval evaluation.
    """
    from deepeval.models import GPTModel

    settings = load_settings()
    eval_settings = settings.get("eval", {})

    # Set API key from settings if provided
    if api_key := eval_settings.get("api_key"):
        os.environ["OPENAI_API_KEY"] = api_key

    return GPTModel(
        model=eval_settings.get("model", "gpt-4o-mini"),
        temperature=eval_settings.get("temperature", 0),
    )


# =============================================================================
# Configuration Dataclass
# =============================================================================


@dataclass
class EvalConfig:
    """Complete evaluation configuration."""

    binary_path: Path
    eval_model: str
    agent_model: str | None
    verbose: bool
    api_tests_enabled: bool

    @classmethod
    def from_env(cls) -> "EvalConfig":
        """Create config from environment and settings.

        Returns:
            EvalConfig instance with all settings loaded.
        """
        return cls(
            binary_path=get_binary_path(),
            eval_model=get_eval_model_name(),
            agent_model=get_agent_model(),
            verbose=is_verbose(),
            api_tests_enabled=is_api_tests_enabled(),
        )


def get_config() -> EvalConfig:
    """Get the current evaluation configuration.

    Returns:
        EvalConfig instance.
    """
    return EvalConfig.from_env()
