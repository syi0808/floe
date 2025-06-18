from orchestrator_agent.base_agent import BaseAgent
import task_agent.task_core as core
from datetime import datetime, timezone
from typing import Optional, List, Dict, Any, Tuple, Literal
from orchestrator_agent.common_types import AgentResponse

# Define TaskStatus type alias based on the literals used in TaskItem.status
TaskStatus = Literal['todo', 'in-progress', 'done', 'archived']
# Define valid status values as a list for validation
VALID_STATUS_VALUES = ['todo', 'in-progress', 'done', 'archived']
import shlex
import uuid # For task_id validation

class TaskAgent(BaseAgent):
    def __init__(self):
        super().__init__()
        self._name = "TaskAgent"

    @property
    def name(self) -> str:
        return self._name

    @property
    def supported_intents(self) -> List[str]:
        return ["task_management"]

    def _parse_due_date(self, due_date_str: str) -> Optional[datetime]:
        """Parses a due date string into a UTC datetime object."""
        try:
            if 'T' in due_date_str: # ISO 8601 format with time
                dt = datetime.fromisoformat(due_date_str.replace('Z', '+00:00'))
            else: # Date only
                dt = datetime.strptime(due_date_str, "%Y-%m-%d")

            if dt.tzinfo is None: # If no timezone, assume UTC for date-only, or convert for datetime
                 # For date-only, it's fine to just attach UTC. For datetime, this might need adjustment
                 # if local times without tz were expected. For now, assume UTC.
                return dt.replace(tzinfo=timezone.utc)
            return dt.astimezone(timezone.utc) # Ensure it's UTC
        except ValueError:
            return None

    def _parse_common_task_attributes(self, args: List[str]) -> Tuple[Dict[str, Any], Optional[str]]:
        """
        Parses common task attributes like priority, due, status, tags.
        Returns a dictionary of attributes and an error message string if any.
        """
        attributes: Dict[str, Any] = {}
        error_message: Optional[str] = None

        i = 0
        while i < len(args):
            arg_lower = args[i].lower()

            if arg_lower == "priority":
                if i + 1 < len(args):
                    try:
                        attributes['priority'] = int(args[i+1])
                        i += 1
                    except ValueError:
                        return {}, f"Error: Invalid priority value '{args[i+1]}'. Priority must be a number."
                else:
                    return {}, "Error: Priority value missing."
            elif arg_lower == "due":
                if i + 1 < len(args):
                    due_date = self._parse_due_date(args[i+1])
                    if due_date is None:
                        return {}, f"Error: Invalid due date format '{args[i+1]}'. Use YYYY-MM-DD or YYYY-MM-DDTHH:MM:SSZ."
                    attributes['due_date_utc'] = due_date
                    i += 1
                else:
                    return {}, "Error: Due date value missing."
            elif arg_lower == "status":
                if i + 1 < len(args):
                    # Get the status value - this will be a single token if quoted in the command
                    # or we'll need to handle multi-word unquoted status
                    status_val = args[i+1].lower()
                    i += 1  # Already consumed the next token
                    
                    # If not already a valid status and might be part of a multi-word status
                    if status_val not in VALID_STATUS_VALUES:
                        # Define keywords that would indicate the end of status value
                        keywords = ["priority", "due", "tags", "tag", "description"]
                        status_words = [status_val]
                        
                        # Continue consuming tokens until we hit a keyword or end of args
                        while i + 1 < len(args) and args[i+1].lower() not in keywords:
                            status_words.append(args[i+1].lower())
                            i += 1
                        
                        # Join all words to form complete status value
                        status_val = " ".join(status_words)
                    
                    # Check if the status value is valid
                    if status_val not in VALID_STATUS_VALUES:
                        return {}, f"Error: Invalid status value '{status_val}'. Allowed: {', '.join(VALID_STATUS_VALUES)}."
                    
                    attributes['status'] = status_val
                else:
                    return {}, "Error: Status value missing."
            elif arg_lower == "tags" or arg_lower == "tag": # project_tag in TaskItem
                if i + 1 < len(args):
                    # Assuming tags are provided as a single string, space-separated
                    # task_core.create_task and update_task expect project_tag as a single string for now.
                    # If multiple tags were stored as List[str] in core, this would need adjustment.
                    # For now, we'll take the first tag if multiple are given in the string,
                    # or the user should quote "tag1 tag2" if that's meant as one tag.
                    # Based on current core.TaskItem, it's `project_tag: Optional[str]`.
                    # So we'll use the provided string as is.
                    attributes['project_tag'] = args[i+1]
                    i += 1
                else:
                    return {}, "Error: Tags value missing."
            elif arg_lower == "description":
                if i + 1 < len(args):
                    attributes['description'] = args[i+1]
                    i += 1
                else:
                    return {}, "Error: Description value missing."
            else:
                # This indicates an argument that wasn't a keyword modifying a previous one.
                # In update, this could be an error. In create, it was assumed part of description.
                # For _parse_common_task_attributes, this is an unknown param.
                return {}, f"Error: Unknown parameter or value out of place: '{args[i]}'."
            i += 1

        return attributes, None

    def _handle_create_task(self, args: List[str], user_id: str) -> AgentResponse:
        """Handles 'add task' or 'create task' commands."""
        if not args:
            return AgentResponse(
                status='error',
                data=None,
                message="Error: No task description provided. Usage: add task \"<description>\" [priority <num>] [due <date>] [tags <tag>]",
                source_agent=self.name
            )

        description = args[0] # First argument is always description
        remaining_args = args[1:]

        attributes, error = self._parse_common_task_attributes(remaining_args)
        if error:
            return AgentResponse(
                status='error',
                data=None,
                message=error,
                source_agent=self.name
            )

        # Set defaults if not provided
        priority = attributes.get('priority', 2)
        due_date_utc = attributes.get('due_date_utc', None)
        project_tag = attributes.get('project_tag', None)
        # Convert single project_tag to project_tags list if provided
        project_tags = [project_tag] if project_tag else None

        try:
            task = core.create_task(
                user_id=user_id,
                description=description,
                priority=priority,
                due_date_utc=due_date_utc,
                project_tags=project_tags
            )
            return AgentResponse(
                status='success',
                data={'task': task.__dict__},
                message=f"Task '{task.description}' created with ID: {task.id}",
                source_agent=self.name
            )
        except ValueError as e:
            return AgentResponse(
                status='error',
                data=None,
                message=f"Error creating task: {e}",
                source_agent=self.name
            )
        except Exception as e:
            return AgentResponse(
                status='error',
                data=None,
                message=f"An unexpected error occurred while creating the task: {e}",
                source_agent=self.name
            )

    def _format_task_details(self, task: core.TaskItem) -> str:
        """Formats a TaskItem into a readable string."""
        lines = [f"Task Details (ID: {task.id}):"]
        lines.append(f"  Description: {task.description}")
        lines.append(f"  Status: {task.status}")
        lines.append(f"  Priority: {task.priority}")
        lines.append(f"  Created: {task.created_at.strftime('%Y-%m-%d %H:%M:%S %Z') if task.created_at else 'N/A'}")
        # TaskItem doesn't have updated_at field in the model, removing it
        lines.append(f"  Due Date: {task.due_date_utc.strftime('%Y-%m-%d %H:%M:%S %Z') if task.due_date_utc else 'N/A'}")
        lines.append(f"  Completed At: {task.completed_at.strftime('%Y-%m-%d %H:%M:%S %Z') if task.completed_at else 'N/A'}")
        lines.append(f"  Project Tags: {', '.join(task.project_tags) if task.project_tags else 'N/A'}")
        return "\n".join(lines)

    def _handle_get_task(self, args: List[str], user_id: str) -> AgentResponse:
        if not args:
            return AgentResponse(
                status='error',
                data=None,
                message="Error: Task ID missing. Usage: get task <task_id>",
                source_agent=self.name
            )
        task_id_str = args[0]
        try:
            # Validate UUID, though core.get_task will also do this
            uuid.UUID(task_id_str)
        except ValueError:
            return AgentResponse(
                status='error',
                data=None,
                message=f"Error: Invalid Task ID format '{task_id_str}'.",
                source_agent=self.name
            )

        task = core.get_task(task_id=task_id_str) # task_core's get_task doesn't take user_id
        if task:
            return AgentResponse(
                status='success',
                data={'task': task.__dict__},
                message=self._format_task_details(task),
                source_agent=self.name
            )
        else:
            return AgentResponse(
                status='error',
                data=None,
                message=f"Task with ID '{task_id_str}' not found or not authorized for user '{user_id}'.",
                source_agent=self.name
            )

    def _handle_update_task(self, args: List[str], user_id: str) -> AgentResponse:
        if not args:
            return AgentResponse(
                status='error',
                data=None,
                message="Error: Task ID missing. Usage: update task <task_id> [description \"...\"] [priority <num>] ...",
                source_agent=self.name
            )

        task_id_str = args[0]
        try:
            uuid.UUID(task_id_str)
        except ValueError:
            return AgentResponse(
                status='error',
                data=None,
                message=f"Error: Invalid Task ID format '{task_id_str}'.",
                source_agent=self.name
            )

        update_args = args[1:]
        if not update_args:
            return AgentResponse(
                status='error',
                data=None,
                message="Error: No update parameters provided. Usage: update task <task_id> description \"...\"",
                source_agent=self.name
            )

        updates, error = self._parse_common_task_attributes(update_args)
        if error:
            return AgentResponse(
                status='error',
                data=None,
                message=error,
                source_agent=self.name
            )

        if not updates:
            return AgentResponse(
                status='error',
                data=None,
                message="Error: No valid fields to update were specified.",
                source_agent=self.name
            )

        try:
            # Ensure completed_at is set if status is changed to 'done'
            if updates.get('status') == 'done' and 'completed_at' not in updates:
                updates['completed_at'] = datetime.now(timezone.utc)
            elif updates.get('status') and updates.get('status') != 'done' and 'completed_at' not in updates:
                # If status is changed to something other than 'done', clear completed_at
                updates['completed_at'] = None

            updated_task = core.update_task(task_id=task_id_str, updates=updates)
            if updated_task:
                return AgentResponse(
                    status='success',
                    data={'task': updated_task.__dict__},
                    message=f"Task '{updated_task.id}' updated successfully.\n{self._format_task_details(updated_task)}",
                    source_agent=self.name
                )
            else:
                # This case might be covered by exceptions in core.update_task
                return AgentResponse(
                    status='error',
                    data=None,
                    message=f"Task with ID '{task_id_str}' not found or failed to update.",
                    source_agent=self.name
                )
        except ValueError as e: # e.g. task not found from core
            return AgentResponse(
                status='error',
                data=None,
                message=f"Error updating task: {e}",
                source_agent=self.name
            )
        except Exception as e:
            return AgentResponse(
                status='error',
                data=None,
                message=f"An unexpected error occurred while updating task: {e}",
                source_agent=self.name
            )

    def _handle_complete_task(self, args: List[str], user_id: str) -> AgentResponse:
        if not args:
            return AgentResponse(
                status='error',
                data=None,
                message="Error: Task ID missing. Usage: complete task <task_id>",
                source_agent=self.name
            )
        task_id_str = args[0]
        try:
            uuid.UUID(task_id_str)
        except ValueError:
            return AgentResponse(
                status='error',
                data=None,
                message=f"Error: Invalid Task ID format '{task_id_str}'.",
                source_agent=self.name
            )

        updates = {
            "status": "done",
            "completed_at": datetime.now(timezone.utc)
        }
        try:
            updated_task = core.update_task(task_id=task_id_str, updates=updates)
            if updated_task:
                return AgentResponse(
                    status='success',
                    data={'task': updated_task.__dict__},
                    message=f"Task '{updated_task.id}' marked as complete.\n{self._format_task_details(updated_task)}",
                    source_agent=self.name
                )
            else:
                return AgentResponse(
                    status='error',
                    data=None,
                    message=f"Task with ID '{task_id_str}' not found or failed to complete.",
                    source_agent=self.name
                )
        except ValueError as e:
            return AgentResponse(
                status='error',
                data=None,
                message=f"Error completing task: {e}",
                source_agent=self.name
            )
        except Exception as e:
            return AgentResponse(
                status='error',
                data=None,
                message=f"An unexpected error occurred while completing task: {e}",
                source_agent=self.name
            )

    def _handle_delete_task(self, args: List[str], user_id: str) -> AgentResponse:
        if not args:
            return AgentResponse(
                status='error',
                data=None,
                message="Error: Task ID missing. Usage: delete task <task_id>",
                source_agent=self.name
            )
        task_id_str = args[0]
        try:
            uuid.UUID(task_id_str)
        except ValueError:
            return AgentResponse(
                status='error',
                data=None,
                message=f"Error: Invalid Task ID format '{task_id_str}'.",
                source_agent=self.name
            )

        if core.delete_task(task_id=task_id_str): # task_core's delete_task doesn't take user_id
            return AgentResponse(
                status='success',
                data={'task_id': task_id_str},
                message=f"Task '{task_id_str}' deleted successfully.",
                source_agent=self.name
            )
        else:
            # core.delete_task might raise an error for not found, or return False
            return AgentResponse(
                status='error',
                data=None,
                message=f"Task with ID '{task_id_str}' not found or not authorized for deletion.",
                source_agent=self.name
            )

    def _handle_list_tasks(self, args: List[str], user_id: str) -> AgentResponse:
        """Handles 'list tasks' or 'show tasks' commands."""
        status_filter: Optional[str] = None
        project_tag_filter: Optional[str] = None

        i = 0
        while i < len(args): # Basic parser for list filters
            arg = args[i].lower()
            if arg == "status":
                if i + 1 < len(args):
                    status_filter = args[i+1]
                    if status_filter not in VALID_STATUS_VALUES and status_filter is not None: # Allow None
                         return AgentResponse(
                             status='error',
                             data=None,
                             message=f"Error: Invalid status value '{status_filter}'. Allowed: {', '.join(VALID_STATUS_VALUES)}.",
                             source_agent=self.name
                         )
                    i += 1
                else:
                    return AgentResponse(
                        status='error',
                        data=None,
                        message="Error: Status value missing for list command.",
                        source_agent=self.name
                    )
            elif arg == "tag" or arg == "project":
                if i + 1 < len(args):
                    project_tag_filter = args[i+1]
                    i += 1
                else:
                    return AgentResponse(
                        status='error',
                        data=None,
                        message="Error: Tag value missing for list command.",
                        source_agent=self.name
                    )
            else:
                 return AgentResponse(
                     status='error',
                     data=None,
                     message=f"Error: Unknown filter '{args[i]}' for list command. Try 'status <val>' or 'tag <val>'.",
                     source_agent=self.name
                 )
            i += 1

        try:
             // … earlier in _handle_list_tasks …

-            # Cast status_filter to ensure it matches the expected literal type
-            if status_filter is not None and status_filter not in VALID_STATUS_VALUES:
-                return AgentResponse(
-                    status='error',
-                    data=None,
-                    message=f"Error: Invalid status value '{status_filter}'. Allowed: {', '.join(VALID_STATUS_VALUES)}.",
-                    source_agent=self.name
-                )
-            # Direct status mapping to ensure type safety
-            valid_status = None
-            if status_filter == 'todo':
-                valid_status = 'todo'
-            elif status_filter == 'in-progress':
-                valid_status = 'in-progress'
-            elif status_filter == 'done':
-                valid_status = 'done'
-            elif status_filter == 'archived':
-                valid_status = 'archived'
-            
-            tasks = core.list_tasks(user_id=user_id, status=valid_status, project_tag=project_tag_filter)
+            tasks = core.list_tasks(
+                user_id=user_id,
+                status=status_filter,
+                project_tag=project_tag_filter
+            )
            tasks = core.list_tasks(user_id=user_id, status=valid_status, project_tag=project_tag_filter)
            if not tasks:
                return AgentResponse(
                    status='success',
                    data={'tasks': []},
                    message="No tasks found matching your criteria.",
                    source_agent=self.name
                )

            response_lines = [f"Found {len(tasks)} task(s):"]
            for task in tasks:
                due_date_str = task.due_date_utc.strftime("%Y-%m-%d") if task.due_date_utc else "N/A"
                line = f"- ID: {task.id}, Desc: \"{task.description}\", Prio: {task.priority}, Due: {due_date_str}, Status: {task.status}"
                # Only use project_tags from the model
                if task.project_tags:
                    line += f", Tags: {', '.join(task.project_tags)}"
                response_lines.append(line)
            
            return AgentResponse(
                status='success',
                data={'tasks': [task.__dict__ for task in tasks]},
                message="\n".join(response_lines),
                source_agent=self.name
            )
        except Exception as e:
            return AgentResponse(
                status='error',
                data=None,
                message=f"An unexpected error occurred while listing tasks: {e}",
                source_agent=self.name
            )

    def process(self, entities: Dict[str, Any], user_id: str) -> AgentResponse:
        # Extract the request from entities or use empty string as fallback
        request = entities.get('request', '')
        
        try:
            parts = shlex.split(request.strip())
        except ValueError as e:
            return AgentResponse(
                status='error',
                data=None,
                message=f"Error parsing request: {e}. Ensure quotes are properly matched.",
                source_agent=self.name
            )

        if not parts:
            return AgentResponse(
                status='error',
                data=None,
                message="Please provide a command.",
                source_agent=self.name
            )

        # Normalize command: combine first two parts if they form a known multi-word command
        raw_command = parts[0].lower()
        potential_multi_word_command = ""
        if len(parts) >= 2:
            potential_multi_word_command = parts[0].lower() + " " + parts[1].lower()

        # Define command mappings to handlers and if they require an ID as the first argument
        # Format: "command phrase": (handler_method_name, needs_id_as_first_arg, is_list_command_alias)
        # needs_id_as_first_arg is True if the command is like "get task <id>", False otherwise
        # is_list_command_alias is True if this command is an alias for listing tasks (e.g. "show tasks")

        # Order matters for prefix commands, e.g. "get task" vs "get tasks"
        # Longer, more specific commands should come first in this lookup if there's ambiguity
        command_map = {
            "add task": (self._handle_create_task, False, False),
            "create task": (self._handle_create_task, False, False),
            "new task": (self._handle_create_task, False, False),

            "list tasks": (self._handle_list_tasks, False, True),
            "show tasks": (self._handle_list_tasks, False, True), # Alias for list
            "get tasks": (self._handle_list_tasks, False, True),  # Alias for list

            "get task": (self._handle_get_task, True, False),     # Needs ID
            "show task": (self._handle_get_task, True, False),    # Needs ID
            "view task": (self._handle_get_task, True, False),    # Needs ID

            "update task": (self._handle_update_task, True, False), # Needs ID
            "edit task": (self._handle_update_task, True, False),   # Needs ID

            "complete task": (self._handle_complete_task, True, False), # Needs ID
            "finish task": (self._handle_complete_task, True, False),   # Needs ID
            "done task": (self._handle_complete_task, True, False),     # Needs ID

            "delete task": (self._handle_delete_task, True, False), # Needs ID
            "remove task": (self._handle_delete_task, True, False)  # Needs ID
        }

        command_to_execute = None
        args_for_handler = []

        # Check for multi-word commands first
        if potential_multi_word_command in command_map:
            command_to_execute = potential_multi_word_command
            args_for_handler = parts[2:]
        elif raw_command in command_map: # Check for single-word commands (if any were defined)
            # This block might not be strictly necessary if all primary commands are multi-word
            # or if single-word commands are distinct enough not to be prefixes of multi-word ones.
            # For now, assuming most primary commands are two words (e.g. "add task")
            # or specific single words that aren't prefixes.
            # If we had "list" as a command, it would conflict with "list tasks" if not handled carefully.
            # The current logic: if "list tasks" matches, "list" alone won't be tried unless "list tasks" isn't found.
            # This is generally okay.
            command_to_execute = raw_command
            args_for_handler = parts[1:]
        else:
             # Fallback for potentially ambiguous "get", "show", "list" if not followed by "tasks" or an ID
             # This is tricky. If someone types "get my_task_id", it should be _handle_get_task
             # If "get" or "list" or "show" are typed alone, what should they do?
             # Current map handles "get task <id>", "list tasks".
             # "get" alone is not defined, "list" alone is not defined.
            return AgentResponse(
                status='error',
                data=None,
                message=f"Sorry, I didn't understand the command '{parts[0]}'. Try commands like 'add task', 'list tasks', 'get task <id>', etc.",
                source_agent=self.name
            )

        handler_method, needs_id, is_list_alias = command_map[command_to_execute]

        # Special handling for "get task", "show task", "list tasks", "show tasks", "get tasks"
        # If the command is an alias for list tasks (e.g. "show tasks") AND there are no arguments,
        # it should proceed to list tasks.
        # If it's "get task" (needs_id=True) and an ID is provided, it should go to _handle_get_task.
        # If it's "get task" but no ID is provided, it's an error.
        # If it's "get tasks" (is_list_alias=True), it goes to _handle_list_tasks.

        if needs_id:
            if not args_for_handler: # ID is missing
                return AgentResponse(
                    status='error',
                    data=None,
                    message=f"Error: Task ID missing for command '{command_to_execute}'. Usage: {command_to_execute} <task_id> [options]",
                    source_agent=self.name
                )
            # For commands like "get task <id>", the ID is the first arg. Handler expects all args.
            return handler_method(args_for_handler, user_id)
        elif is_list_alias:
            # For commands like "list tasks [filters]", handler expects filter args.
            return handler_method(args_for_handler, user_id)
        else: # Create command like "add task <desc> [options]"
            # The first arg is description, not an ID.
            return handler_method(args_for_handler, user_id)

        # This part should ideally not be reached if the map and logic are correct
        return AgentResponse(
            status='error',
            data=None,
            message=f"Sorry, I'm having trouble understanding '{request}'. Please try again.",
            source_agent=self.name
        )
