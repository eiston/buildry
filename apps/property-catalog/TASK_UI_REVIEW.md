# Task UI review

Reviewed October 31, 2026 in a new built-in browser tab using isolated mock data.

## Changes

- Reused FormButton for task actions and added its standard form association prop.
- Separated Save from Plan task, Start work, and Complete task. Saving an edit no
  longer advances status; successful saves have a concise status message.
- Added loading and unavailable-task screens. Missing task IDs no longer render
  an editable blank form.
- Task rows show the assigned person's name and active schedule.
- Reused DatePicker for scheduling. Schedule fields sit together on desktop and
  stack on phones. Assignee selection waits for an available house/manager.
- Mobile filter controls retain readable labels and scroll within their toolbar.
  The task search field remains available at intermediate tablet widths.
- Removed redundant empty-state instructions. No explanatory subtitles were added.
- The shared dialog handler now includes alert dialogs: deletion confirmation
  traps keyboard focus, closes with Escape, and restores focus to Delete task.
- Task uploads use the shared dropzone with a task-specific accessible label.

## Validation

Screens checked at 1440 × 1000 and 390 × 844:

- Task register and search, matching/no-result filters, empty list and API error.
- New task form, backlog, planning, in-progress, completed, and aborted tasks.
- Missing task link, tenant-request task, description editor, photo viewer, and
  deletion confirmation.

Workflow checks passed for task creation, native required-field validation,
house/manager selection, schedule saving, saving edits without advancing status,
starting work, completion, matching/no-result search, combined house/assignee
filters, filter clearing, photo upload/view/removal, invalid-file rejection,
description editing, and delete-dialog keyboard focus/dismissal. The delete
operation was not submitted.

The route audit records 22 desktop/mobile cases without page-wide horizontal
overflow. Additional 320px editor and 720px list checks passed. Final browser
console checks had no warnings or errors. The production Docker build,
JavaScript syntax check, and `git diff --check` passed. No server logic changed;
the server unit suite was not rerun. The mock preview does not verify production
authorization, storage, or server workflow constraints. Physical file dragging
was not simulated; the file picker was exercised.

## Preview

With the Compose app running:

```sh
python3 scripts/preview-tasks.py
```

Open http://127.0.0.1:8082/tasks. All task writes and image upload responses stay
in memory; no existing database tasks are changed. Restarting restores fixtures.
Use `?preview=empty` or `?preview=error` on the task list for those states.

Screenshots and the route/overflow audit are saved in:

`/Users/eiston.wei/.codex/visualizations/2026/10/31/01a19357-c9ca-71a2-8a28-a86a67e3d035/tasks/`
