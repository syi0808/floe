# orchestrator_agent/calendar_adapters/__init__.py

import os
import platform # For platform check for Apple Calendar
from typing import Optional # Import Optional

from .base_adapter import CalendarAdapter
from .google_calendar_adapter import GoogleCalendarAdapter
from .apple_calendar_adapter import AppleCalendarAdapter

# Environment variable to determine which calendar backend to use
CALENDAR_BACKEND_ENV_VAR = "CALENDAR_BACKEND"
DEFAULT_CALENDAR_BACKEND = "google" # Default to Google if not specified

def get_calendar_adapter(backend_type: Optional[str] = None) -> CalendarAdapter:
    """
    Factory function to get an instance of a calendar adapter.

    Args:
        backend_type (Optional[str]): The type of calendar backend to use
            (e.g., "google", "apple"). If None, it tries to read from
            the CALENDAR_BACKEND_ENV_VAR environment variable. Defaults to
            DEFAULT_CALENDAR_BACKEND if the env var is not set.

    Returns:
        CalendarAdapter: An instance of the requested calendar adapter.

    Raises:
        ValueError: If an unsupported calendar backend type is specified.
        RuntimeError: If the Apple Calendar adapter is selected on a non-macOS platform.
    """
    if backend_type is None:
        backend_type = os.environ.get(CALENDAR_BACKEND_ENV_VAR, DEFAULT_CALENDAR_BACKEND).lower()
    else:
        backend_type = backend_type.lower()

    print(f"Calendar Adapter Factory: Selected backend type: '{backend_type}'")

    if backend_type == "google":
        # Credentials/token paths for GoogleCalendarAdapter can be passed here if they
        # are not hardcoded or if they come from a central config.
        # For now, GoogleCalendarAdapter uses defaults like "credentials.json".
        return GoogleCalendarAdapter()
    elif backend_type == "apple":
        if platform.system() != "Darwin":
            raise RuntimeError("Apple Calendar adapter can only be used on macOS.")
        # Default calendar name for AppleCalendarAdapter can be configured here if needed.
        return AppleCalendarAdapter()
    # Add other adapters here as they are implemented
    # elif backend_type == "outlook":
    #     return OutlookCalendarAdapter()
    else:
        raise ValueError(f"Unsupported calendar backend type: {backend_type}")

__all__ = [
    "CalendarAdapter",
    "GoogleCalendarAdapter",
    "AppleCalendarAdapter",
    "get_calendar_adapter",
]
