#!/bin/bash

# Goal: Create a new markdown document summarizing TaskAgent work and next steps.

# Read the content of task_agent_remaining_work.txt
if [ -f "task_agent_remaining_work.txt" ]; then
    TASK_AGENT_WORK_CONTENT=$(cat task_agent_remaining_work.txt)
else
    echo "Error: task_agent_remaining_work.txt not found." >&2
    exit 1
fi

# Read the content of next_development_steps.txt
if [ -f "next_development_steps.txt" ]; then
    NEXT_STEPS_CONTENT=$(cat next_development_steps.txt)
else
    echo "Error: next_development_steps.txt not found." >&2
    exit 1
fi

# Define the output filename
OUTPUT_FILENAME="docs/task_agent_work_summary.md"

# Create the new markdown file with the combined content
cat << EOF > $OUTPUT_FILENAME
# Summary of TaskAgent Status and Next Development Steps

This document summarizes the remaining work for the \`TaskAgent\` and outlines the subsequent development focus for the Floe AI Assistant. The information is derived from \`docs/remaining_work_plan.md\`.

## Remaining Work for \`TaskAgent\`

The following tasks are pending for the completion of the \`TaskAgent\` implementation:

$TASK_AGENT_WORK_CONTENT

## Next Development Focus Post-\`TaskAgent\`

Once the \`TaskAgent\` is complete, development will proceed with the following agents and key areas:

$NEXT_STEPS_CONTENT

---
*Source: \`docs/remaining_work_plan.md\`*
EOF

# Verify the file was created and has content
if [ -s "$OUTPUT_FILENAME" ]; then
    echo "Successfully created $OUTPUT_FILENAME"
    # Print the content of the new file to standard output for verification
    cat $OUTPUT_FILENAME
else
    echo "Error: Failed to create or populate $OUTPUT_FILENAME" >&2
    exit 1
fi
