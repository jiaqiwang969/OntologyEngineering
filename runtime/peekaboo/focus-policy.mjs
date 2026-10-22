// Disable only the provider's synthetic focus override. Never change native window focus.
import { registerHooks } from "node:module";
import { createHash } from "node:crypto";
import fs from "node:fs";
import { fileURLToPath } from "node:url";

const reviewedSource = "d49b2666cb9c6c5b4d7b9130235c476cb8a3af580363ff224796d9dbde99538d";
registerHooks({
  load(url, context, nextLoad) {
    const result = nextLoad(url, context);
    if (!url.endsWith("/chrome-devtools-mcp/build/src/McpPage.js")) return result;
    const source = Buffer.from(result.source).toString("utf8");
    if (createHash("sha256").update(source).digest("hex") !== reviewedSource) {
      throw new Error("Chrome DevTools MCP focus behavior changed; review the provider before browser operations");
    }
    const needle = "this.pptrPage.emulateFocusedPage(true)";
    if (source.split(needle).length !== 2) throw new Error("Provider focus override identity changed");
    const patched = source.replace(needle, "this.pptrPage.emulateFocusedPage(false)");
    const receipt = process.env.OEV2_PEEKABOO_PROVIDER_RECEIPT;
    if (!receipt) throw new Error("Owned provider pin receipt path is required");
    const proof = {source_path:fileURLToPath(url),original_sha256:reviewedSource,
      patched_sha256:createHash("sha256").update(patched).digest("hex"),synthetic_focus_enabled:false};
    // Only this worker's private receipt; provider installation is never edited.
    fs.writeFileSync(receipt, JSON.stringify(proof), {mode:0o600});
    return { ...result, source: patched };
  },
});
