# Shared UI components

The calendar/tour and task review changes now compose these reusable components:

| Component | Responsibility | Consumers |
| --- | --- | --- |
| `Dialog` / `ConfirmDialog` | Desktop modal, mobile sheet, close/busy semantics, keyboard focus and dismissal; optional swipe header | Tour details, tour cancellation, task deletion, photo viewer |
| `TourTimePicker` | Shared DatePicker, available starts, pressed state, loading/no-times/error states | Host and guest rescheduling |
| `PageState` | Concise loading, empty, error, and missing-record content with caller-owned recovery actions | Task list/search/details, guest booking management |
| `PhotoGallery` / `PhotoViewer` | Thumbnails, opening/removal, optional cover/reorder events, image selection and full-screen view | Property/room PhotoManager and task photos |
| `FilterBar` | Existing shared selectors, responsive layout, and Clear action | Task register and search |
| `ScheduleSummary` | Consistent date/time/duration presentation | Task rows/details and guest/host tour details |
| `ButtonVariant` | Primary, outline, danger, icon, and text appearances on FormButton | Shared components and migrated task/tour actions |

Booking availability, API calls, task transitions, upload handling, reorder/remove
mutations, and unsaved state remain in their owning pages. Existing property cover
and hold-to-drag markup/callbacks are retained. Non-sortable galleries no longer
activate the property's drag handler. The viewer scrolls to the chosen image
again as preceding images finish loading to avoid image-layout shifts.

No default helper paragraphs or explanatory subtitles are introduced. Existing
FormInput, FormSelect, FormMultiSelect, DatePicker, AppForm, FormCard, FormActionBar,
and UploadDropzone remain the underlying controls.

## Verification

Production frontend compilation and local browser review use the isolated fixtures
from `scripts/preview-tours.py` (8081) and `scripts/preview-tasks.py` (8082). These
preview servers never forward API writes to the real app. Gallery fixtures also
support property review at `/properties/1`.

Browser coverage includes task saving without status advancement, modal keyboard
focus/Escape/restoration, host and guest rescheduling, property/task photo viewing,
property photo removal/undo, filters, missing records, and desktop/mobile layouts.
Physical hold-to-drag and touch-swipe gestures are not simulated.

The pushed frontend source snapshot is built separately from the working tree to
check that it does not depend on the uncommitted server/deployment changes. Earlier
server, deployment, and server-rendered document changes are left in the working
tree for their owning work. The branch includes the earlier shared-control and
calendar/task frontend improvements required by these components.

Screenshot and DOM audit artifacts:
`/Users/eiston.wei/.codex/visualizations/2026/10/31/01a19357-c9ca-71a2-8a28-a86a67e3d035/shared/`
