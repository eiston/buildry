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

The initial frontend-only source snapshot was built separately from the working
tree to check that it did not depend on the remaining server/deployment edits.
The November 1, 2026 integration into main includes those server, deployment,
document-script, and review changes as well as the shared-control and calendar/task
frontend improvements. Combined production frontend/API compilation and all 36
server tests passed. JavaScript syntax checks, Compose configuration validation,
and whitespace checks also passed.

Screenshot and DOM audit artifacts:
`/Users/eiston.wei/.codex/visualizations/2026/10/31/01a19357-c9ca-71a2-8a28-a86a67e3d035/shared/`

## Clock picker and borderless form cards

`TimePicker` uses an analog clock face with clickable hour/minute numbers, direct
hour/minute switching, AM/PM buttons, and one-minute adjustment buttons. Selecting
an hour moves to minutes. Both modes have the same panel height. Set time applies
the draft; Cancel, close, and Escape discard it. Exact minute values are retained
and stored as `HH:MM`. The picker reuses Dialog, FormButton, Icon, and field context.

The shared form-card rules cover date/time triggers, native data-entry fields,
textareas, selectors, and multi-selects. They use borderless, transparent rows;
checkboxes, radios, range controls, and upload surfaces retain their semantics.
Popover surfaces and keyboard focus indicators remain separate from data fields.

Decorative date/time icons, selector chevrons, and selected-value avatar ornaments
are hidden inside shared form cards. Action icons and option-list content remain
available. Native date/time indicators and number-field spinners are also hidden;
keyboard editing and field validation remain intact.

Final built-in-browser review captured 18 task/picker states at 1440×1000,
390×844, and 320×740, plus the property form. Hour/minute panels kept identical
heights at each width, with no clipping or horizontal overflow. Shared task and
property fields had no borders or visible decorative icons. Save/reload checks
covered exact minutes, noon, and midnight; Cancel and Escape preserved the saved
value, and Escape restored focus to the trigger. Arrow-key minute adjustment
passed. The production frontend/API build and all 36 server tests passed again.

Form-card spacing is shared through `--field-label-gap`, `--field-value-top`, and
`--field-value-line`. Text, number, inline text, address, date/time, select, and
multi-select values use the notebook preview's 5px label gap and 4px top padding,
with a 19.6px first line. Single-line controls retain a 44px target in both display
and edit states. Desktop (1440px) and mobile (390px) task/property screenshots and
computed-style checks confirm consistent spacing; selectors and the clock still
open correctly. Production frontend and API compilation passed for this CSS change.

Single-line value areas now use `--field-value-height:44px` for fixed height in
editable, disabled, and display-only states. `FormValue` reuses `FormField` for
property ownership and task house/title/schedule displays; text descriptions keep
a multiline variant. Selected names stay on one line. Browser measurements at
1440px, 390px, and 320px confirm equal single-line control and row heights, including
inline editing, while text areas remain 96px and galleries keep their own layout.

`FormRows` owns contiguous full-width highlight surfaces within `FormCard`.
Field spacing lives in row padding, with zero grid gap or vertical row margins;
consecutive groups inside `AppForm` also meet directly. Desktop hover screenshots
and desktop/mobile geometry checks show adjacent row boundaries touching, including
House→Assignee and Assignee→schedule. Property rows follow the same global layout.
Reviewed 1440px, 390px, and 320px widths with no horizontal overflow; the mobile
selector still opens correctly. Production frontend/API compilation passed.

Select rendering review found that closing destroyed every option node and keyboard
focus applied the global 0.48s animated drop-shadow. Keyboard opening also waited
two animation frames, while option navigation called page-scrolling APIs.
`FormSelect` and `FormMultiSelect` now mount lazily once and retain hidden options
between opens. Hidden menus/backdrops remain out of layout and keyboard navigation;
row stacking follows aria-expanded. Keyboard focus waits for actual menu visibility,
uses a static outline, and scrolls only the option container. Browser review covered
first/repeated opens, selection, multi-select search, Escape/focus restoration, and
long year-list End navigation. The latter scrolled the list while page scrollY
remained 243. Computed option animation/filter are now none. Desktop and mobile
screenshots were inspected; production frontend/API build, JS syntax, and whitespace
checks passed. No frame-rate benchmark was collected.

The retained-menu follow-up fixes card stacking: only a select menu without the
hidden attribute elevates its card. Previously any mounted menu kept z-index 80
after closing and covered the mobile navigation (z-index 20). Reproduced the failure,
then checked backdrop/Escape/selection closes at 390px and multi-select Done at
320px: card z-index returns to auto, and all five navigation hit-test points resolve
to the sidebar. Clicking Tasks navigates successfully. Desktop multi-select open/
close also passes. Updated both preview stylesheet paths from the final source;
production frontend/API build and whitespace checks passed.
