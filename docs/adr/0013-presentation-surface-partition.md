---
status: accepted
title: One realization owner per presentation surface
summary: Presentation splits into a thin assembly coordinator and one realization owner per surface over shared assets and style.
keywords:
  - presentation-surface
  - ownership
---

# One realization owner per presentation surface

`app/src/presentation.rs` presented every runtime surface — the play frame, the title, the pause overlay with its slot grid, Settings, the Gallery and the Timeline — from one 3,918-line module with no interface between them. The decision splits it into a thin assembly coordinator (`app/src/presentation.rs`) and one realization owner per surface — `play`, `title`, `overlay`, `settings_view`, `gallery`, `timeline` — over shared `assets` and `style` support. Each surface draws its own builders and syncs; the coordinator alone owns the one rebuild signature (`MenuBuilt`), the root despawn/rebuild transaction and the single focus repaint; and derived presentation state (`AppliedAudio`, `AppliedVolume`, `AppliedVisuals`, `AppliedUi`, `TextReveal`, `SettingsDrag`, `AssetWatch`, the embedded faces) stays with the owner that realizes it, in separately stored ECS resources behind concrete operations. The menu keeps navigation, focus, confirmation and origins; the Monitor reader keeps its shared row metrics; session recipes keep the choice of *when* an entry or replacement replays or resets, while the play owner implements *how* ([ADR 0012](0012-menu-and-live-session-transition-ownership.md)).

A single owned presentation aggregate was rejected because it coarsens Bevy borrowing and change detection and hides the per-surface rebuild conditions. One module per door was rejected because Settings and Gallery are one implementation reached from two doors, with the door passed in as a render input rather than a second code path. Fusing each surface's systems into one frame sync was rejected because the menu tree rebuilds only on a structural signature, the play chain's order is load-bearing, and the post-layout and content systems carry the required schedule edges the shipping-schedule ambiguity gate protects.

Plan-only at recording: the behaviour-preserving extraction is a separate implementation handoff, due at the hardening map's close. The partition, its interfaces and its validation contract are in [Runtime hardening — Presentation surfaces](../runtime-hardening.md#presentation-surfaces--decided-partition). No new domain concept, no public API, no persistence change. **Delivered (2026-10-15; presentation-state ownership tightened 2026-10-16):** the recording-time "plan-only" status no longer applies; the decision itself is unchanged.
