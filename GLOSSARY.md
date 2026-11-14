# unmei-engine

A minimal Bevy-based visual-novel runtime where NPC responses are chosen at play time by OpenRouter Decisions, plus an offline authoring CLI. This glossary covers the vocabulary shared by the runtime, the CLI, and authored packs.

## Language

### Content & packs

**Content pack**:
A directory in the pack layout (manifest, state, scenes, npcs, assets) holding everything one game needs to play.
_Avoid_: game folder, mod, project

**Manifest**:
The pack's `pack.ron`: pack name, entry scene, and the optional `tagline`, `title_backdrop` and ordered `scenes` list.
_Avoid_: pack config, header

**Default pack**:
The `demo-pack` content pack in the folder that holds the player executable, played when no pack directory is given. Never resolved from the working directory.
_Avoid_: bundled pack (when meaning the lookup rule), `examples/demo-pack` (the repository copy)

**Entry scene**:
The scene the runtime loads first, named by the manifest.

**Declared scene order**:
The manifest's optional ordered `scenes` list — the pack's declared play order and the only order any consumer reads. Absent or empty (`scenes: []`) means no declared order: the pack plays as today with no timeline, and no order is derived.
_Avoid_: chapter list, scene index

### Scripts

**Scene**:
One `.unmei` file of dialogue and commands; its id is its filename stem.
_Avoid_: script file, chapter, level

**chapter** (retired alias):
An older name for a scene, not a separate content unit. Use **Scene** for the content and **Timeline** for the scene-selection surface.

**Say-line**:
A line of the form `Speaker: text` — the only construct not introduced by a keyword.
_Avoid_: dialogue line, text line

**Run**:
A maximal contiguous block of say-lines between non-say-line lines (a command, a flow line, or a scene end) — the unit a single `unmei author draft` invocation rewrites.
_Avoid_: passage, block, segment

**Label**:
A named position in a scene that `jump` targets; names may contain dots (e.g. `reply.warm`). Labels are scene-local.
_Avoid_: marker, anchor

**Player choice**:
A `choice`/`end` block presenting options the player picks between; each option targets a label.
_Avoid_: menu, branch

**Conditional**:
An `if` / `else` / `endif` block in a scene that plays one of two line groups based on the state map.
_Avoid_: branch (the `choice` avoid-list already claims it), if-statement

**Condition**:
The `<state key> <op> <literal>` test a conditional evaluates.
_Avoid_: predicate, expression

**State map**:
The flat key-value map declared in `state.ron`, mutated only by `set`, referenced by `{var}` interpolation and by decision requests. Keys are declared before use.
_Avoid_: variables, game state, save data

### Saving

**Sidecar**:
A file beside the pack directory, named after it — `<pack-dir>.<suffix>`. The save slots, the read log, the settings file and the session log are sidecars; the credential file in the user config directory is deliberately not one, so a shared pack directory never carries the key. The naming rule and the atomic write are one shared idiom, not one per class.
_Avoid_: companion file, metadata file, save-adjacent file

**Save slot**:
One of the four fixed save files beside the pack — three manual slots the player writes and one autosave the game writes.
_Avoid_: save file, savegame, checkpoint

**Autosave**:
The game-written save slot, refreshed at a scene crossing, a resolved player choice or a model-answered decision; an authored fallback alone does not refresh it. Load-only, never a save or import target.
_Avoid_: quicksave, checkpoint

**Legacy save**:
The slice's single `<pack-dir>.save.ron`, offered once in the overlay for import into a manual slot, then renamed aside.
_Avoid_: old save, migration file

**Unreadable slot**:
A save slot that refuses to load — its bytes fail to decode, its version is newer than the build, its IO fails, it names a scene the pack lacks, or it fails a semantic check (a `pc` outside its scene, a wait parked at `pc == 0`, an undeclared state key, a wait that does not match its line). Never auto-deleted, and still overwritable.
_Avoid_: corrupt save, broken save

### Reading

**Read log**:
The pack-adjacent `<pack-dir>.read.ron` recording what the player has read in one pack: the say-lines seen, keyed by scene and authored source line, and the scenes visited. Player knowledge — not a save and not a preference: one per pack, versioned, never auto-deleted, and New Game deliberately does not reset it; an unusable file means everything unread rather than an error.
_Avoid_: progress file, seen-text database, completion tracker

**Visited scene**:
A scene the player has played in — entered with New Game or Continue, crossed into by a jump, or restored by a load — recorded in the read log.
_Avoid_: unlocked scene, completed scene, opened scene

**Arrival status**:
Whether the say-line at the player's current park was read *before* this visit: captured on the park transition, before that frame's mark lands, and held for that park's duration only — never across a park change, a load, a New Game or a Timeline jump. It is what the seen skip reads, so the line on screen is skippable only when it was already read.
_Avoid_: read state, first-visit flag, seen flag

**Skip**:
The in-play fast-forward: one step every `SKIP_DWELL_SECONDS` (0.05 s) that completes any reveal, holds the line whole and advances. It only ever advances say-lines, never passes a choice or decision wait or a scene end, and never chooses, resolves or writes anything but the read log's own marks. Two controls: a `Tab`-toggled seen skip and a held `Ctrl` force skip.
_Avoid_: fast-forward, FF, auto-advance

**Seen skip**:
The `Tab`-toggled skip, gated by the read log: it can only run while the parked line was read before this visit, and it ends at the first unread say-line, at any park that is not a say-line, and at a load, a New Game or a Timeline jump. Any player input ends it.
_Avoid_: read-tracked skip, safe skip

**Force skip**:
The held-`Ctrl` skip: every say-line, read or not, holds at any park it cannot pass and resumes while the key is held. It writes no read marks, but the scenes it crosses are still visited.
_Avoid_: hard skip, brute skip, FF

### Pause & menu

**Menu overlay**:
The single Esc-toggled pause surface — the card grid of save slots (plus the one-time Legacy save card), the Settings view holding audio and player-comfort settings, the gallery of the pack's declared images, and the Timeline of its declared scenes.
_Avoid_: pause menu, options screen, main menu

**Presentation pause**:
What Esc does: freezes player input and shows the menu without advancing the script or cancelling an in-flight Decisions request; a completed decision answer is held until resume.
_Avoid_: game pause, suspend, halt

**Input ownership**:
The app's single boundary for who receives player input: the ordinary app surfaces unless a topmost claimant takes the frame, in which case play/menu/shortcut dispatch, focused editing, clipboard paste and Settings drag are suppressed — while physical release tracking (an already-held skip key) keeps running.
_Avoid_: input mode, focus grab, capture

**Title screen**:
The gated boot surface shown after a successful pack load and reachable again from Settings: Continue, New Game, Settings, Gallery and Quit, over a play path frozen exactly as the menu overlay freezes it.
_Avoid_: main menu, start menu, home screen

**Gallery**:
A fourth view on the pause surface: it shows the pack's declared images as a grid of cards, one of which can be shown full-window, and is opened from the title screen's `Gallery` row and the Esc overlay's top bar; opening it changes nothing about the session.
_Avoid_: asset browser, image viewer, art book

**Timeline**:
The pause surface listing a pack's scenes in its declared order, with visited scenes selectable and unvisited scenes hidden as `???`. Selecting a visited scene starts it from the beginning with the current state and staging carried over — not a historical rewind.
_Avoid_: chapter timeline, chapter select, scene history

**Backlog**:
The contents page's scrollback of the live session: the say-lines shown, the choices picked and a `<npc_id> decided` marker per resolved decision, newest last, at most 200. Held in memory only, cleared when New Game, a load or a Timeline jump replaces the position. Not the read log, which records what was ever read, not the order.
_Avoid_: history, log screen, rollback

**Still view**:
The gallery's one sub-view: a single entry shown full-window over the gallery's backdrop, with its label as the caption; `←`/`→` browse the entries and `↑`/`↓`, `Esc` or a click return to the grid. Inert — it never advances the script or writes anything.
_Avoid_: lightbox, viewer, full-screen mode

**Gallery entry**:
One image a pack declares: a background id named by a `bg` line or by the manifest's `title_backdrop`, or a sprite `(npc, pose)` pair named by a `sprite` line. Repeated declarations of the same image are one entry, and an entry is shown under its own label — a background's raw id, or a sprite's `npc · pose` pair.
_Avoid_: asset, thumbnail, card (the card is the surface that draws an entry)

**Placeholder frame**:
What a gallery card and the still draw for an entry whose image is missing or undecodable, and while its file is still loading: the art box with a faint 1px solid outline (Bevy 0.19's UI has no dashed-border primitive) and the entry's label beneath it, with no marker, glyph or Notice; the failure reaches the log once per path under the background/sprite rows.
_Avoid_: fallback image, error frame, missing-art icon

**Title backdrop**:
The optional full-window image a pack supplies behind the title screen, named by the manifest's `title_backdrop` and resolved exactly like a scene background (`assets/backgrounds/<id>.png`); without one the neutral gradient stands, and a missing or undecodable file degrades to it silently.
_Avoid_: title art, splash image, hero image

**Tagline**:
The optional one-line pack-supplied copy under the title screen's wordmark, carried by the manifest's `tagline`; a pack writes natural case and the title renders it uppercased.
_Avoid_: subtitle, blurb, strapline

**Live session**:
The in-memory play state since the player entered play this launch — resumed by the title's Continue, frozen while the title or the menu overlay is up, cleared by app exit; not a save slot and never persisted.
_Avoid_: run, playthrough, game session

### Failures

**Dev mode**:
The app-lifetime developer affordance policy shared by Monitor and the F5/F9 manual-slot-1 shortcuts, separate from the Monitor's visibility and from player save/load controls.
_Avoid_: debug menu, developer settings, Monitor mode

**Monitor**:
The developer-only, in-window viewer of the current app launch's diagnostic log stream, reachable over any runtime surface without introducing a presentation pause — not the player-facing Notice or a performance-telemetry surface.
_Avoid_: console, output logger, debug menu

**Monitor line**:
One diagnostic event as presented by the Monitor, irrespective of how its text wraps on screen.
_Avoid_: console row, physical line, log record

**Monitor admission**:
A completed Monitor line's insertion into the pending completed queue — not the earlier formatter reservation — where its monotonic identity, elapsed time and any truncation are stamped; ordered newest-last with no cross-producer causal order.
_Avoid_: reservation time, callback start order, timestamp sort

**Monitor handoff staging**:
Completed Monitor lines removed from the pending queue for ingestion but still charged against the 256-slot pending cap until retained history owns each payload.
_Avoid_: uncharged staging, second pending buffer

**Monitor projection**:
The Monitor's bounded display view over the one shared retained history — the exact case-sensitive `unmei::` targets by default, or every surviving admitted event under **Show everything** — never a second buffer, a capture reset or a filter on the capture itself.
_Avoid_: log copy, second buffer, upstream filter

**Monitor reading anchor**:
The top visible retained event's admission identity plus the within-row reading offset a non-following Monitor reader holds, so arrivals, hiding/reopening, a filter refresh and a rewrap never move the reader or resurrect evicted text. The offset is measured against the anchor row's actual laid-out extent (its per-column wrapping), so the stored top-visible event matches the rendered reading position rather than a guessed line height. Its loss is acknowledged in place as **`older lines expired`**. While a restarted projection is still locating the anchor, the reader is in its **locating state**: the body shows a local `locating your position…` placeholder under `updating view` instead of presenting the newest matching rows as the anchor's location.
_Avoid_: scroll position, line number, pixel offset, second history

**Held-input quarantine**:
The keys and pointer buttons held during a Monitor ownership frame, remembered until each is physically released and pressed again, so a held `Space`/`Enter` cannot type or commit and a held click cannot activate underneath after the drawer closes. The already-armed pre-Monitor `Ctrl` force hold is the one deliberate exception: physical release tracking stays live while the drawer owns input.
_Avoid_: input freeze, key lock, input reset

**Notice**:
The single transient line at the top right for the failures that lose the player's data or leave the game unable to move; it appears once per failure, lasts a few unpaused seconds, replaces rather than queues, and never consumes input.
_Avoid_: toast, popup, error message

**Silent degradation**:
A failure that changes what the player sees or hears but never what they lose — a missing or undecodable asset, no audio device, a pack warning — logged and deliberately never shown.
_Avoid_: soft failure, handled error

**Defensive failure**:
A failure branch the pack loader's validation makes unreachable, specified anyway because its alternative is a frozen screen: an error log, a Notice that says so, and the menu as the only affordance.
_Avoid_: impossible error, unreachable branch

**Error screen**:
The startup surface that stops play before it starts — for a pack that will not load, the pack path, up to twenty load errors and a pointer at the pack's session log (`<pack-dir>.log`), plus the default-pack hint when no `PACK_DIR` was given; or, when the executable cannot locate itself, the one message `stderr` carries and no log pointer at all. Esc or a click quits.
_Avoid_: crash screen, fatal dialog

### Settings

**Settings file**:
The pack-adjacent `<pack-dir>.settings.ron` holding player preferences — the audio mix (master, music, SFX, mute) and the play group (text speed, auto, fullscreen). Sectioned, unversioned, and one per pack.
_Avoid_: config file, preferences file, options file

**Credential file**:
The user-level `credentials` file holding the player's OpenRouter API key — `%APPDATA%/unmei/credentials` on Windows, `~/.config/unmei/credentials` on Linux. The one sidecar that is not a pack sibling, so a shared pack directory never carries the key; the `OPENROUTER_API_KEY` environment variable overrides it.
_Avoid_: config file, key file, secrets file

**Text speed**:
The player's reveal pace for say-lines, stored as seconds per character in `play.text_speed`; `0.0` (the default) means no reveal at all.
_Avoid_: typewriter speed, scroll speed

**Auto mode**:
The `play.auto` preference that advances say-lines by itself, 2.0 s after a line is fully revealed; it holds at choices, decision points and scene ends. While it is on, a dim in-play `AUTO` cue shows it.
_Avoid_: autoplay, auto-advance (the cue is named after the setting)

**Decisions group**:
The Settings view group holding the key row, after the audio and play groups.
_Avoid_: key settings, API section

**Key row**:
The row in the Decisions group that owns the player's key state — Not set / Set / Set, not checked / Checking… / Invalid / Invalid, rejected by OpenRouter / From environment — and the only place a key can be entered or cleared. Its verdicts are live for the session, never persisted.
_Avoid_: key field, api field

### Authoring

**Draft**:
To generate one run's prose lines with the authoring model, offline in the CLI. v1 drafts prose only — never control flow, never commands.
_Avoid_: generate (reserved for decision configs), write, compose

**Preparation failure**:
A staged-validation failure before the candidate is loader-validated — the temporary directory, a transplant source read, the staged read/parse, serialization, or the write. The command stops terminally (exit 1, no retry, no correction, no further attempt, no interactive feedback prompt), distinct from a loader **content rejection**, which is correction material. It governs NPC decision generation; the Scene-drafting path's I/O-as-corrections classification is a separate, still-open policy.
_Avoid_: staging error, validation error (that is the loader's content verdict)

**Terminal mask**:
The author CLI's replacement of every exact occurrence of a non-empty `UNMEI_API_KEY` or `OPENROUTER_API_KEY` value with the fixed `[redacted]` marker on stdout/stderr for the whole command — argument/help rendering, final errors, `--list`, previews, dry runs, exhaustion reports, interactive prompts and dependency logging. It is a literal match, not a secret scanner, and it does not touch generated files, provider prompts or correction payloads; a masked preview does not make a pack safe to share.
_Avoid_: redaction filter (implies content awareness), safe-output guarantee

### Decisions

**NPC**:
A non-player character with a structured definition file (identity, action lists, decision configs). The player is not an NPC.
_Avoid_: character (includes the player), actor

**Question type**:
The `type` discriminator on a decision config — `choice` (the default), `noul`, or `score` — which fixes the shape of `criteria` and the landing key the answer produces.
_Avoid_: question kind, mode

**Noul question**:
A decision config answering yes/no; the response's `noul` probability `>= 0.5` lands the script on `<prefix>.true`, else `<prefix>.false`.
_Avoid_: boolean question, yes/no question

**Score question**:
A decision config answering one of an ordered list of levels; the answer lands the script on `<prefix>.<legend index>`.
_Avoid_: rating, scale question, gauge

**Legend**:
The ordered list of level descriptions of a score question, lowest first; its `0`-based positions are the landing keys.
_Avoid_: scale, buckets

**Action**:
An id in a `choice` config's criteria map — both a Decisions answer and a script branch target.
_Avoid_: response, option, line

**Decision point**:
A `decide <npc>.<question> -> <prefix>` line in a scene: one Decisions call whose answer lands the script on `<prefix>.<landing key>` — an action id, `true`/`false`, or a legend index, depending on the question type.
_Avoid_: choice point (confusable with player choice), decision node

**Landing key**:
The suffix a decision answer appends to the `decide` line's prefix to name the label to jump to — an action id (`choice`), `true`/`false` (`noul`), or a legend index (`score`).
_Avoid_: answer key, result key

**Criterion**:
A description inside a decision config's `criteria` that the decision model judges the state against — an action's in `choice`, a level's in `score`, `true`/`false` in `noul`.
_Avoid_: option description, answer text

**Fallback**:
The landing key used when the Decisions call fails or answers unusably — a criterion id (`choice`), `true`/`false` (`noul`), or a legend index (`score`). The engine logs and continues offline.
_Avoid_: fallback action (a bool or index is not an action), default (ambiguous with author default action)

**Decision resolver**:
The injection point through which the runtime asks for a parked question's resolution, with the author fallback already applied — `resolve(...) -> ResolvedDecision`. `DecisionsClient` is its one real implementation; a test implements it locally to drive the app's decision pipeline without a network. It is not a provider abstraction (ADR 0009).
_Avoid_: `DecisionBackend` (a provider-agnostic trait, declined), provider (the resolver is not the vendor), fallback handler (the in-fiction fallback is a config value, not an implementation)

**Model pin**:
The decision model named for an NPC (`npc.model`) and sent as the request's `model`. A successful response's echoed `model` is trusted as provenance only when it matches the pin exactly; otherwise the engine reports the fixed marker `unverified response model`. It names what was requested, never proof of the responding build, and the echoed score legend is never trusted provenance either.
_Avoid_: responding model (unverified), exact model (a mismatch is possible), provider model

### Distribution

**Release version**:
The one SemVer version a beta release ships under — shared by both crates, both executables, the archive names and the release tag. It identifies a release, not a stored format.
_Avoid_: build number, app version, format version

**Player archive**:
The downloadable per-OS archive that holds the player executable with the default pack, a short player README and the license texts. A stranger plays from its extracted folder.
_Avoid_: player zip (Linux ships a tarball), bundle, installer

**CLI archive**:
The downloadable per-OS archive that holds the author CLI with a short author README, the license texts and a copy of the demo pack.
_Avoid_: CLI zip, toolkit

**Guide**:
The task-oriented pages that walk players, authors and contributors through Unmei and link into the specs without restating them. A reader of a release follows the Guide as it stood at that release's tag.
_Avoid_: wiki, on-ramp, tutorial site

**Format version**:
The version written inside a stored file (a save slot or the read log) that decides whether a build can read it. It changes only when that file's format changes, independently of the release version.
_Avoid_: save version (when meaning the release), release version
