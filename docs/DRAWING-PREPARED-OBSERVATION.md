# Prepared drawing observation

Explicit display intent/3 and params/3 separate one legacy context preparation
from repeated observation/7. The shared Native detailed reader retains the old
/6 behavior for existing callers. In /7 it queries the already displayed part,
current drawing, display mode and work-view type; it never sets the display
part, opens drawings, changes display mode or updates views. Wrong or unknown
context is an error, with no preparation fallback.

Uncalled setters have null return values and explicit called=false. Original
query codes, geometry, complete sheet inventory, dimension associations and
source bytes still undergo the existing checks. Preparation is retained in the
execution report and independently bound to the same part, sheet and original
references. Legacy PDF/1, identified/1 and display/1–2 keep their contracts.

Display/3 retains /2 short labels, five phase inventories and guarded original
WCS restoration. The final complete display inventory must equal the original;
removing context reentry does not relax identity, geometry or restoration checks.
Cleanup still attempts all original restoration steps after partial failure.
The CAD source and output copy are never saved in this transient PDF operation.

The motivating native failures localize WCS change to detailed readback,
which contains three context-setting calls. This change tests removal of that
bundle. It does not establish which individual SDK call caused the change.
Mac/Windows software, actual NX, native restoration, PDF parsing and visual
review have separate receipts. A source candidate is not native qualification.

Owner: oe-nx-worker drafting/drawing_pdf/drawing_identification/drawing_display,
with existing oe-cad receiver and Project/work/Harness. The installed NX 2412
SDK supplies the query/setter ABI; no legacy cad-agent execution is used.
Independent tests cover false setter success, wrong/changed contexts, protocol
mixing, actual call counts and partial-failure cleanup. Real original-source
trials and source/PDF tamper controls remain required before receiving.

All 63 goals, CAD16, complete Rust Semantica, engineering sources, manufacturing
and inspection, and each original project acceptance remain. PDF readability
and execution restoration do not establish manufacturing or physical metrology.
