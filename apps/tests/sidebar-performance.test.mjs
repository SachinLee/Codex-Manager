import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import test from "node:test";

const appsRoot = path.resolve(import.meta.dirname, "..");

test("sidebar collapse avoids transitions that continuously reflow the active page", async () => {
  const source = await fs.readFile(
    path.join(appsRoot, "src", "components", "layout", "sidebar.tsx"),
    "utf8",
  );

  assert.match(source, /isSidebarOpen \? "w-60" : "w-16"/);
  assert.match(
    source,
    /data-slot="app-sidebar"[\s\S]{0,220}flex shrink-0 flex-col glass-sidebar/,
  );
  assert.doesNotMatch(source, /transition-\[width\]/);
  assert.doesNotMatch(source, /transition-all/);
  assert.doesNotMatch(source, /transition-\[clip-path\]/);
  assert.doesNotMatch(source, /will-change:clip-path/);
  assert.doesNotMatch(source, /app-sidebar-motion-(?:layer|surface)/);
  assert.match(
    source,
    /data-slot="app-sidebar-motion-edge"[\s\S]{0,300}transition-transform/,
  );
});

test("main content exposes a stable layout target for sidebar regression checks", async () => {
  const source = await fs.readFile(
    path.join(appsRoot, "src", "components", "layout", "app-frame.tsx"),
    "utf8",
  );

  assert.match(source, /data-slot="app-main-column"/);
});

test("page fallback stays aligned with both sidebar widths", async () => {
  const source = await fs.readFile(
    path.join(
      appsRoot,
      "src",
      "components",
      "layout",
      "page-keep-alive-viewport.tsx",
    ),
    "utf8",
  );

  assert.match(source, /isSidebarOpen \? "left-60" : "left-16"/);
});
