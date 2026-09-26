# Purchased-part CAD acquisition

Default supplier route is [MISUMI China](misumi-cn-guide.md). Work from engineering
requirements to a complete purchasable part number, then bind its configuration to
the actual CAD and BOM. A visually similar generic model is a declared proxy.

Use existing verified local artifacts first. Otherwise finish parameter selection
in the official product page before generating the CAD. Confirm model/format,
observe a completed local download and inspect its contents; a preview, click or
success banner is not an artifact. Public static URLs may be reused only when
observed in the actual official download, not reverse-engineered from another SKU.

Maintain the supplier intake template at `assets/supplier-intake.template.json`:
requirements and candidate rationale; product URL and full configured identifier;
selected parameters with units; requested/generated format; timestamp; local path,
bytes and SHA-256; units, exact/proxy representation, critical interface checks;
NX import/save/reopen identity; BOM definition and occurrence/pack quantities;
dated quantity-specific price, shipping/arrival terms and procurement status.
Each state remains separate: selected, downloaded, geometry-verified,
interface-accepted, supply-confirmed, ordered. Unknowns are not filled with defaults.

Never replace a required purchased fastener, bearing, guide or actuator with a
coarse envelope while retaining an exact-manufacturing claim. Simplified supplier
B-Rep may omit internal geometry; document the fit/clearance scope it actually
supports. Imported supplier CAD does not establish load rating, preload, backlash,
friction, material or life; bind those to the current catalog/drawing/test evidence.

Use direct NXOpen/Journal to create native copies with provenance and save/reopen
readback. External translator licenses/formats must be checked on the actual NX
installation. No Fusion catalog, McMaster API or CAD MCP server is a prerequisite.
Legacy McMaster guidance and the user-provided Fusion menu image remain at
[mcmaster-entry-guide.md](mcmaster-entry-guide.md) for historical reference only.
