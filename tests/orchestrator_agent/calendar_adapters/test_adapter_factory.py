# Unit tests for orchestrator_agent.calendar_adapters.__init__ (factory)
import pytest
import unittest
from unittest.mock import patch, MagicMock
import platform # To control platform.system() output

# Modules to test
from orchestrator_agent.calendar_adapters import get_calendar_adapter, CalendarAdapter
from orchestrator_agent.calendar_adapters.google_calendar_adapter import GoogleCalendarAdapter
from orchestrator_agent.calendar_adapters.apple_calendar_adapter import AppleCalendarAdapter

class TestAdapterFactory(unittest.TestCase):

    @patch('os.environ.get')
    def test_get_google_adapter_default(self, mock_env_get):
        # Test when CALENDAR_BACKEND is not set, defaults to google
        mock_env_get.return_value = "google" # or None, and it defaults
        adapter = get_calendar_adapter()
        self.assertIsInstance(adapter, GoogleCalendarAdapter)
        mock_env_get.assert_called_once_with("CALENDAR_BACKEND", "google")

    @patch('os.environ.get')
    def test_get_google_adapter_explicit_env(self, mock_env_get):
        # Test when CALENDAR_BACKEND is explicitly "google"
        mock_env_get.return_value = "google"
        adapter = get_calendar_adapter()
        self.assertIsInstance(adapter, GoogleCalendarAdapter)

    def test_get_google_adapter_explicit_arg(self):
        # Test when backend_type arg is "google"
        adapter = get_calendar_adapter(backend_type="google")
        self.assertIsInstance(adapter, GoogleCalendarAdapter)

    @patch('platform.system', return_value="Darwin") # Mock platform to be macOS
    @patch('os.environ.get')
    def test_get_apple_adapter_on_mac_env(self, mock_env_get, mock_platform_system):
        mock_env_get.return_value = "apple"
        # Mock AppleCalendarAdapter's connect method to avoid actual osascript check during factory test
        with patch('orchestrator_agent.calendar_adapters.apple_calendar_adapter.AppleCalendarAdapter.connect', return_value=True):
            adapter = get_calendar_adapter()
        self.assertIsInstance(adapter, AppleCalendarAdapter)
        # Factory calls it once, AppleCalendarAdapter.__init__ calls it again.
        self.assertEqual(mock_platform_system.call_count, 2)

    @patch('platform.system', return_value="Darwin")
    def test_get_apple_adapter_on_mac_arg(self, mock_platform_system):
        with patch('orchestrator_agent.calendar_adapters.apple_calendar_adapter.AppleCalendarAdapter.connect', return_value=True):
            adapter = get_calendar_adapter(backend_type="apple")
        self.assertIsInstance(adapter, AppleCalendarAdapter)
        # Factory calls it once, AppleCalendarAdapter.__init__ calls it again.
        self.assertEqual(mock_platform_system.call_count, 2)

    @patch('platform.system', return_value="Linux") # Mock platform to be Linux
    @patch('os.environ.get')
    def test_get_apple_adapter_fail_not_mac_env(self, mock_env_get, mock_platform_system):
        mock_env_get.return_value = "apple"
        with self.assertRaisesRegex(RuntimeError, "Apple Calendar adapter can only be used on macOS."):
            get_calendar_adapter()
        # Factory calls it once to check, then error is raised.
        mock_platform_system.assert_called_once()

    @patch('platform.system', return_value="Windows")
    def test_get_apple_adapter_fail_not_mac_arg(self, mock_platform_system):
        with self.assertRaisesRegex(RuntimeError, "Apple Calendar adapter can only be used on macOS."):
            get_calendar_adapter(backend_type="apple")
        # Factory calls it once to check.
        mock_platform_system.assert_called_once()

    @patch('os.environ.get')
    def test_get_adapter_invalid_backend_env(self, mock_env_get):
        mock_env_get.return_value = "outlook" # Assume outlook is not yet supported
        with self.assertRaisesRegex(ValueError, "Unsupported calendar backend type: outlook"):
            get_calendar_adapter()

    def test_get_adapter_invalid_backend_arg(self):
        with self.assertRaisesRegex(ValueError, "Unsupported calendar backend type: unknown_calendar"):
            get_calendar_adapter(backend_type="unknown_calendar")

    @patch('os.environ.get')
    def test_get_adapter_case_insensitivity(self, mock_env_get):
        mock_env_get.return_value = "GOOGLE" # Uppercase
        adapter = get_calendar_adapter()
        self.assertIsInstance(adapter, GoogleCalendarAdapter)

        adapter_arg = get_calendar_adapter(backend_type="Google") # Mixed case
        self.assertIsInstance(adapter_arg, GoogleCalendarAdapter)


if __name__ == '__main__':
    unittest.main()
