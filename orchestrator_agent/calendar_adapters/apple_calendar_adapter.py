# orchestrator_agent/calendar_adapters/apple_calendar_adapter.py
import subprocess
import re
from datetime import datetime, timedelta, timezone
from typing import Tuple, Optional, List, Dict, Any
import shlex
import platform # For platform check in connect

# CalendarEvent is imported from .base_adapter below
from .base_adapter import CalendarAdapter
from task_agent.task_calendar_linker import CalendarEvent

# --- Constants ---
FLOE_EVENT_ID_KEY = "FLOE_EVENT_ID"
FLOE_TASK_ID_KEY = "FLOE_TASK_ID"
FLOE_ID_SEPARATOR = "::"
DEFAULT_APPLE_CALENDAR_NAME = "Calendar" # Default calendar if none specified

# Helper to get event properties in a structured way from AppleScript
# Returns properties as a comma-separated string: "id,summary,startDate,endDate,description"
# Uses ISO 8601 for dates.
_EVENT_PROPERTIES_SCRIPT_FORMAT = """
set output to ""
set anID to id of theEvent
set aSummary to summary of theEvent
set aStartDate to (start date of theEvent as «class isot» as string)
set aEndDate to (end date of theEvent as «class isot» as string)
set aDescription to description of theEvent
if aDescription is missing value then set aDescription to ""
set output to anID & "," & aSummary & "," & aStartDate & "," & aEndDate & "," & aDescription
return output
"""

class AppleCalendarAdapter(CalendarAdapter):
    def __init__(self, default_calendar_name: str = DEFAULT_APPLE_CALENDAR_NAME):
        self.default_calendar_name = default_calendar_name
        self._is_macos = (platform.system() == "Darwin")

    def _execute_applescript(self, script: str) -> Tuple[bool, str, str]:
        if not self._is_macos:
            return False, "", "Apple Calendar adapter only works on macOS."
        try:
            process = subprocess.run(
                ["osascript", "-e", script],
                capture_output=True, text=True, check=False, timeout=15
            )
            return process.returncode == 0, process.stdout.strip(), process.stderr.strip()
        except FileNotFoundError:
            return False, "", "osascript command not found."
        except subprocess.TimeoutExpired:
            return False, "", "AppleScript execution timed out."
        except Exception as e:
            return False, "", f"An unexpected error occurred during AppleScript execution: {e}"

    def _datetime_to_applescript_date(self, dt: datetime) -> str:
        return f'date "{dt.strftime("%A, %B %d, %Y at %H:%M:%S")}"'

    def _parse_applescript_datetime_str(self, as_date_str: str) -> Optional[datetime]:
        if not as_date_str: return None
        try:
            if as_date_str.endswith("Z"):
                 dt = datetime.fromisoformat(as_date_str.replace("Z", "+00:00"))
            else:
                 dt = datetime.fromisoformat(as_date_str)
            return dt
        except ValueError:
            try:
                dt = datetime.strptime(as_date_str, "%A, %B %d, %Y at %H:%M:%S")
                return dt
            except ValueError as e_fallback:
                print(f"Error parsing AppleScript date string '{as_date_str}': {e_fallback}.")
                return None

    def _extract_floe_id_from_description(self, description: Optional[str], id_key: str) -> Optional[str]:
        if not description or not id_key: return None
        match = re.search(rf"{re.escape(id_key)}{re.escape(FLOE_ID_SEPARATOR)}([^\n]+)", description)
        return match.group(1).strip() if match else None

    def _build_description_with_floe_ids(self, original_description: Optional[str], floe_event_id: str, task_id_ref: Optional[str]) -> str:
        clean_description = original_description if original_description else ""
        clean_description = re.sub(rf"{re.escape(FLOE_EVENT_ID_KEY)}{re.escape(FLOE_ID_SEPARATOR)}[^\n]*\n?", "", clean_description)
        clean_description = re.sub(rf"{re.escape(FLOE_TASK_ID_KEY)}{re.escape(FLOE_ID_SEPARATOR)}[^\n]*\n?", "", clean_description)
        clean_description = clean_description.strip()
        id_lines = []
        if floe_event_id: id_lines.append(f"{FLOE_EVENT_ID_KEY}{FLOE_ID_SEPARATOR}{floe_event_id}")
        if task_id_ref: id_lines.append(f"{FLOE_TASK_ID_KEY}{FLOE_ID_SEPARATOR}{task_id_ref}")
        id_block = "\n".join(id_lines)
        return f"{id_block}\n{clean_description}" if clean_description else id_block

    def _escape_applescript_string(self, value: Optional[str]) -> str:
        if value is None: return ""
        return shlex.quote(value).strip("'") # Suitable for AppleScript context

    def _parse_event_properties_string(self, props_str: str) -> Optional[Dict[str, Any]]:
        try:
            parts = props_str.split(',', 4)
            if len(parts) == 5:
                return {
                    "apple_native_id": parts[0], "summary": parts[1],
                    "start_date_str": parts[2], "end_date_str": parts[3],
                    "description": parts[4]
                }
            return None
        except Exception as e:
            print(f"Error parsing event properties string '{props_str}': {e}")
            return None

    def connect(self) -> bool:
        if not self._is_macos:
            print("Apple Calendar adapter cannot connect: Not running on macOS.")
            return False
        success, _, err = self._execute_applescript("return true")
        if not success:
            print(f"Failed to verify osascript functionality for Apple Calendar: {err}")
            return False
        print("Apple Calendar adapter connected (osascript available).")
        return True

    def create_event(self, event_data: CalendarEvent, calendar_target: Optional[str] = None) -> Optional[str]:
        calendar_name = calendar_target or self.default_calendar_name
        desc_with_ids = self._build_description_with_floe_ids(
            event_data.description, event_data.event_id, event_data.task_id_ref
        )
        script = f"""
        tell application "Calendar"
            tell calendar "{self._escape_applescript_string(calendar_name)}"
                set theStartDate to {self._datetime_to_applescript_date(event_data.start_time)}
                set theEndDate to {self._datetime_to_applescript_date(event_data.end_time)}
                set theNewEvent to make new event with properties {{summary:"{self._escape_applescript_string(event_data.summary)}", start date:theStartDate, end date:theEndDate, description:"{self._escape_applescript_string(desc_with_ids)}"}}
                return id of theNewEvent
            end tell
        end tell"""
        success, out, err = self._execute_applescript(script)
        if success and out:
            print(f"Apple Calendar: Event '{event_data.summary}' created. Native ID: {out}")
            return event_data.event_id
        else:
            print(f"Apple Calendar: Error creating event '{event_data.summary}': {err} (stdout: {out})")
            return None

    def _find_event_applescript_data_by_floe_id(self, floe_event_id: str, calendar_name: str) -> Optional[Tuple[str, Dict[str, Any]]]:
        search_pattern = f"{FLOE_EVENT_ID_KEY}{FLOE_ID_SEPARATOR}{floe_event_id}"
        script = f"""
        set foundEventDetails to ""
        tell application "Calendar"
            tell calendar "{self._escape_applescript_string(calendar_name)}"
                set matchingEvents to (every event whose description contains "{self._escape_applescript_string(search_pattern)}")
                if count of matchingEvents > 0 then
                    set theEvent to item 1 of matchingEvents
                    {_EVENT_PROPERTIES_SCRIPT_FORMAT}
                    set foundEventDetails to output
                end if
            end tell
        end tell
        return foundEventDetails"""
        success, out, err = self._execute_applescript(script)
        if success and out:
            properties = self._parse_event_properties_string(out)
            if properties: return properties["apple_native_id"], properties
            print(f"Apple Calendar: Found event by Floe ID '{floe_event_id}' but failed to parse properties: {out}")
        elif not success:
            print(f"Apple Calendar: Error finding event by Floe ID '{floe_event_id}': {err} (stdout: {out})")
        return None

    def get_event(self, floe_event_id: str, calendar_target: Optional[str] = None) -> Optional[CalendarEvent]:
        calendar_name = calendar_target or self.default_calendar_name
        find_result = self._find_event_applescript_data_by_floe_id(floe_event_id, calendar_name)
        if find_result:
            _apple_native_id, props = find_result
            start_time = self._parse_applescript_datetime_str(props["start_date_str"])
            end_time = self._parse_applescript_datetime_str(props["end_date_str"])
            if not start_time or not end_time: return None
            task_id_ref = self._extract_floe_id_from_description(props["description"], FLOE_TASK_ID_KEY)
            original_desc = props["description"]
            original_desc = re.sub(rf"{re.escape(FLOE_EVENT_ID_KEY)}{re.escape(FLOE_ID_SEPARATOR)}[^\n]*\n?", "", original_desc)
            original_desc = re.sub(rf"{re.escape(FLOE_TASK_ID_KEY)}{re.escape(FLOE_ID_SEPARATOR)}[^\n]*\n?", "", original_desc).strip()
            return CalendarEvent(event_id=floe_event_id, summary=props["summary"], start_time=start_time,
                                 end_time=end_time, description=original_desc or None, task_id_ref=task_id_ref)
        return None

    def update_event(self, floe_event_id: str, event_data: CalendarEvent, calendar_target: Optional[str] = None) -> bool:
        calendar_name = calendar_target or self.default_calendar_name
        find_result = self._find_event_applescript_data_by_floe_id(floe_event_id, calendar_name)
        if not find_result: return False
        apple_native_id, _ = find_result
        new_desc = self._build_description_with_floe_ids(event_data.description, floe_event_id, event_data.task_id_ref)
        script = f"""
        tell application "Calendar"
            tell calendar "{self._escape_applescript_string(calendar_name)}"
                set theEvent to (first event whose id is "{self._escape_applescript_string(apple_native_id)}")
                if theEvent exists then
                    set summary of theEvent to "{self._escape_applescript_string(event_data.summary)}"
                    set start date of theEvent to {self._datetime_to_applescript_date(event_data.start_time)}
                    set end date of theEvent to {self._datetime_to_applescript_date(event_data.end_time)}
                    set description of theEvent to "{self._escape_applescript_string(new_desc)}"
                    return "success"
                end if; return "error: event not found by native id"
            end tell
        end tell"""
        success, out, err = self._execute_applescript(script)
        if success and out == "success": return True
        print(f"Apple Calendar: Error updating Floe ID '{floe_event_id}': {err} (stdout: {out})")
        return False

    def delete_event(self, floe_event_id: str, calendar_target: Optional[str] = None) -> bool:
        calendar_name = calendar_target or self.default_calendar_name
        find_result = self._find_event_applescript_data_by_floe_id(floe_event_id, calendar_name)
        if not find_result: return False # Or True if already gone
        apple_native_id, _ = find_result
        script = f"""
        tell application "Calendar"
            tell calendar "{self._escape_applescript_string(calendar_name)}"
                set theEvent to (first event whose id is "{self._escape_applescript_string(apple_native_id)}")
                if theEvent exists then
                    delete theEvent; return "success"
                end if; return "error: event not found by native id for deletion"
            end tell
        end tell"""
        success, out, err = self._execute_applescript(script)
        if success and out == "success": return True
        if "event not found" in str(out) or "event not found" in str(err): return True # Idempotent
        print(f"Apple Calendar: Error deleting Floe ID '{floe_event_id}': {err} (stdout: {out})")
        return False

    def list_events(self, calendar_target: Optional[str] = None,
                    time_min: Optional[datetime] = None, time_max: Optional[datetime] = None,
                    floe_task_id: Optional[str] = None) -> List[CalendarEvent]:
        calendar_name = calendar_target or self.default_calendar_name
        date_filter_script = "set allEvents to every event"
        if time_min and time_max:
            date_filter_script = f"set allEvents to (every event whose start date ≥ {self._datetime_to_applescript_date(time_min)} and end date ≤ {self._datetime_to_applescript_date(time_max)})"
        elif time_min:
            date_filter_script = f"set allEvents to (every event whose start date ≥ {self._datetime_to_applescript_date(time_min)})"
        elif time_max:
            date_filter_script = f"set allEvents to (every event whose end date ≤ {self._datetime_to_applescript_date(time_max)})"

        script = f"""
        set collectedEventDetails to {{}}
        tell application "Calendar"
            tell calendar "{self._escape_applescript_string(calendar_name)}"
                {date_filter_script}
                repeat with theEvent in allEvents
                    {_EVENT_PROPERTIES_SCRIPT_FORMAT}
                    set end of collectedEventDetails to output
                end repeat
            end tell
        end tell
        set AppleScript's text item delimiters to "|||"
        set detailsString to collectedEventDetails as string
        set AppleScript's text item delimiters to ""
        return detailsString"""
        success, out, err = self._execute_applescript(script)
        found_events: List[CalendarEvent] = []
        if success and out:
            for prop_str in out.split("|||"):
                if not prop_str.strip(): continue
                props = self._parse_event_properties_string(prop_str)
                if props:
                    event_floe_id = self._extract_floe_id_from_description(props["description"], FLOE_EVENT_ID_KEY)
                    if not event_floe_id: continue # Only Floe-managed events
                    event_task_id = self._extract_floe_id_from_description(props["description"], FLOE_TASK_ID_KEY)
                    if floe_task_id and event_task_id != floe_task_id: continue
                    start_time = self._parse_applescript_datetime_str(props["start_date_str"])
                    end_time = self._parse_applescript_datetime_str(props["end_date_str"])
                    if not start_time or not end_time: continue
                    original_desc = props["description"]
                    original_desc = re.sub(rf"{re.escape(FLOE_EVENT_ID_KEY)}{re.escape(FLOE_ID_SEPARATOR)}[^\n]*\n?", "", original_desc)
                    original_desc = re.sub(rf"{re.escape(FLOE_TASK_ID_KEY)}{re.escape(FLOE_ID_SEPARATOR)}[^\n]*\n?", "", original_desc).strip()
                    found_events.append(CalendarEvent(event_id=event_floe_id, summary=props["summary"], start_time=start_time,
                                                     end_time=end_time, description=original_desc or None, task_id_ref=event_task_id))
        elif not success:
            print(f"Apple Calendar: Error listing events: {err} (stdout: {out})")
        return found_events

# --- Main block for basic testing (Updated) ---
if __name__ == "__main__":
    print("Apple Calendar Adapter - Direct Execution Test")
    adapter = AppleCalendarAdapter(default_calendar_name="Test Floe Calendar")
    if adapter.connect():
        print("Adapter connected.")
        # Test CRUD (similar structure to Google adapter's main, but using Apple methods)
        unique_id_suffix = datetime.now().strftime("%Y%m%d%H%M%S%f")
        test_floe_event_id = f"floe_atest_{unique_id_suffix}"
        test_floe_task_id = f"task_atest_{unique_id_suffix}"

        start_time = datetime.now(timezone.utc) + timedelta(minutes=5) # AppleScript might be picky with very near times
        if start_time.tzinfo is None : start_time = start_time.astimezone()


        event_to_create = CalendarEvent(
            event_id=test_floe_event_id,
            summary="Test Apple Event",
            start_time=start_time,
            end_time=start_time + timedelta(hours=1),
            description="Testing AppleCalendarAdapter.",
            task_id_ref=test_floe_task_id
        )
        print(f"\n1. CREATE (Floe ID: {test_floe_event_id})")
        created_id = adapter.create_event(event_to_create) # Uses default calendar
        if created_id:
            print(f"CREATE successful. Returned Floe ID: {created_id}")
            print(f"\n2. GET (Floe ID: {test_floe_event_id})")
            retrieved = adapter.get_event(test_floe_event_id)
            if retrieved: print(f"GET successful. Summary: '{retrieved.summary}'")
            else: print("GET failed.")

            print("\n3. LIST (by task_id)")
            listed = adapter.list_events(floe_task_id=test_floe_task_id)
            print(f"LIST found {len(listed)} event(s) for task {test_floe_task_id}")
            assert any(e.event_id == test_floe_event_id for e in listed)

            print("\n4. UPDATE (Floe ID: {test_floe_event_id})")
            updated_data = CalendarEvent(event_id=test_floe_event_id, summary="Test Apple Event - UPDATED",
                                         start_time=start_time + timedelta(minutes=10),
                                         end_time=start_time + timedelta(hours=1, minutes=10),
                                         description="Desc updated.", task_id_ref=test_floe_task_id)
            update_ok = adapter.update_event(test_floe_event_id, updated_data)
            print(f"UPDATE successful: {update_ok}")
            if update_ok:
                retrieved_updated = adapter.get_event(test_floe_event_id)
                if retrieved_updated : print(f"Verified updated summary: {retrieved_updated.summary}")


            print(f"\n5. DELETE (Floe ID: {test_floe_event_id})")
            delete_ok = adapter.delete_event(test_floe_event_id)
            print(f"DELETE successful: {delete_ok}")
            if delete_ok:
                assert adapter.get_event(test_floe_event_id) is None
                print("Event not found after delete (verified).")
        else:
            print("CREATE failed. Skipping other tests.")
    else:
        print("Adapter connection failed. Cannot run tests.")
    print("\n--- AppleCalendarAdapter Test Run Complete ---")
