---
status: accepted
title: Menu decisions and live-session recipes have separate owners
summary: Menu navigation stays in the menu, and session entry and replacement recipes move to a separate app-owned module.
keywords:
  - ownership
  - presentation-surface
---

# Menu decisions and live-session recipes have separate owners

The menu's transition function mixes navigation and confirmation with coordinated live-session replacement, while the developer load shortcut needs the same replacement without a menu. The decision puts navigation, focus and surface changes in the menu and entry/replacement recipes in a separate app-owned session module, called synchronously through restricted borrowed operations. Existing ECS resources remain separately stored: a session operation owns the cancellation/reset/replay recipe, not every receiver's representation. The menu deliberately retains its single private launch-entry marker, updated only after successful play entry; it is not a second owner of the runner.

An owned session aggregate was rejected because it demands broad resource/public-`Runtime` migration and coarser Bevy borrowing/change detection. A pure menu decision/effect/executor protocol was rejected because its operation/completion vocabulary and round trip are unnecessary for the current synchronous callers. Borrowed access is acceptable only when it hides the recipe: callers cannot cancel tasks or manipulate reset/replay fields themselves. Applied presentation state stays presentation-owned; session recipes request invalidation through narrow concrete access, and the presentation-partition decision must compose with this seam rather than duplicate ownership.

Plan-only at recording; the maintainer confirmed the complete decision on 2026-10-13. The existing pause/input/persistence ordering and failure meanings remain binding. Separate contract repairs precede extraction; detailed interface sketches, repair sequencing and the validation contract are in [Runtime hardening — Menu and live-session transitions](../runtime-hardening.md#menu-and-live-session-transitions--ownership-design). No new session domain concept or persistent format is introduced. **Delivered (2026-10-15):** the contract repairs landed first and the extraction followed; the plan-only wording above records the recording-time status and the decision itself is unchanged.
