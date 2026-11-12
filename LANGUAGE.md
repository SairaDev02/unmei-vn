---
title: Language standards for reader-facing docs
summary: Write and review a project's reader-facing docs, and report findings in one format.
version: 7.0.0
---
# Language standards for reader-facing docs

This page sets the rules for the texts a stranger reads. The rules apply to every reader-facing text in a project, and P1 lists all of them. A project adopts this page by filling in § 2 Project settings. A writer applies the rules while drafting. A reviewer, human or agent, applies each rule to a target text and reports findings in the format in § 8.

These rules assume English-language docs. Docs in another language need their own word lists for A10, B7, B8, D4, D10, D15, D16, D17 and the § 7 searches.

This page restates no contract. When a rule here conflicts with a project's contracts (§ 4), those sources win. Report the conflict as a finding with the label Authority and severity Major (§ 8).

This page uses Semantic Versioning. § 10 states what each kind of version change means and lists the versions.

## 1. How to adopt this page

1. Copy this page into the project's docs.
2. Fill in every row of § 2. Write "none" for a setting that does not apply. Do not delete a row.
3. Once § 2 is complete, link this page from the project's agent instructions and contributor guide. This order keeps readers from meeting empty settings.

A rule that refers to an empty setting cannot be applied. The reviewer reports it as such (§ 8).

## 2. Project settings

| ID | Setting | What to record | Value |
| --- | --- | --- | --- |
| P1 | Texts in scope | Every reader-facing text in the project, the one job it does, the one P2 audience it serves, how that reader reads it (following steps, looking up a fact or reading through), the spec or section that assigns that job and whether it is public. | Public unless marked. `README.md`: what Unmei is and where each reader starts, with the controls table; visitor; reading through. `docs/guide/README.md`: the Guide home, routing each reader to a Guide page; visitor; looking up. `docs/guide/play-the-demo.md`: get, launch and play the demo; player; following steps. `docs/guide/live-decisions.md`: add an OpenRouter key so decision points are live, with what each request sends and costs; player; following steps. `docs/guide/troubleshooting.md`: fix a failed launch or load and report a bug; player; looking up. `docs/guide/install-the-cli.md`: install the CLI and give it a model; author; following steps. `docs/guide/write-your-first-pack.md`: change the demo pack, validate it and play it; author; following steps. `docs/guide/author-with-the-cli.md`: worked `unmei author` flows; author; following steps. `docs/guide/format-overview.md`: a map of a pack's parts, each linked to its content-format section; author; looking up. `docs/architecture.md`: crate boundaries, data flows and the contract map; contributor; reading through. `docs/release-runbook.md`: release a version from the public repository; maintainer; following steps. `docs/testing.md`: which tests each crate needs and which checks a change passes before it is done; contributor; looking up. `docs/maintaining.md`: gate a pull request, triage an issue and answer a reporter; maintainer; following steps. `packaging/player-README.md`: run the demo from the player archive; player; following steps; public inside the archive. `packaging/author-README.md`: run the CLI from the author archive; author; following steps; public inside the archive. `engine/README.md` and the root rustdoc in `engine/src/lib.rs`: what the engine crate offers and where its docs are; crate user; reading through; public on crates.io and docs.rs. `cli/README.md` and the root rustdoc in `cli/src/main.rs` and `cli/src/lib.rs`: what the CLI crate does and how to install it; crate user; reading through; public on crates.io and docs.rs. `.github/PULL_REQUEST_TEMPLATE.md`, the contract-map checkbox only: contributor; following steps. `CONTRIBUTING.md`: how to report an issue and propose a change; contributor; following steps. `SECURITY.md`: how to report a vulnerability and which versions get fixes; visitor; looking up. Not public: the first-time Guide dry-run record, a maintainer evidence record; maintainer; reading through; G1 to G4 do not apply. Each entry assigns its own text: the row states the job in words, so no spec or scope number records the assignment. Every text in scope is in English. |
| P2 | Audiences | Each kind of reader, and what each one is assumed to have and not have. Example: "a contributor knows the language but not this codebase." | Visitor: has a browser and has never seen Unmei; does not yet know whether they will play, author or contribute. Player: has never seen Unmei and has no Rust toolchain or OpenRouter key. Author: writes a content pack; may have no Rust toolchain and no API key. Contributor: knows Rust but not this workspace. Maintainer: releases Unmei from the public repository with the access the runbook names. Crate user: arrives from crates.io or docs.rs; knows Rust and Cargo but not Unmei. |
| P3 | Automated checks | Each command that checks docs, and what it covers. | `python3 scripts/docs.py check`: front matter (title, summary, keywords) in the indexed `docs/` folders, the keyword allowlist, index freshness, the classification of every tracked Markdown path, and that each relative link target and `#anchor` exists in `README.md`, `docs/architecture.md`, `docs/release-runbook.md` and `docs/guide/*.md`. `cargo doc -p unmei-engine --features network --no-deps`: rustdoc warnings in the engine root docs. The packaging refusal in `scripts/package.py`: any leftover `@@TOKEN@@` other than `@@VERSION@@` in packaged texts and crate manifests. |
| P4 | Sources of authority | Where the glossary, specs and decision records live. The glossary is required, so this row is never "none". With no specs or decision records, the reference page or code that defines each area's behavior. | `GLOSSARY.md` (§ Language holds each term and its _Avoid_ list). The specs in `docs/` (`content-format.md`, `author-cli.md`, `runtime-hardening.md` and the others `docs/INDEX.md` lists). The decision records in `docs/adr/`. These own every behavioral fact and each text's job; no spec number records the assignment. |
| P5 | Names | The product name, and each component, package and binary name exactly as written. The one page that owns them. | Product: Unmei. Packages: `unmei-engine` (the engine crate), `unmei` (the CLI crate and binary), `unmei-app` (the Bevy player crate), whose binary is `unmei-play`. Owner: `docs/architecture.md`. The README may keep its one-line crate summary. |
| P6 | Platforms | Each supported platform and install method, and what is not offered (a platform, an installer, a signed build). | Windows x64 and Linux x64 archives for the player and the author CLI. The CLI also installs with `cargo install --locked unmei` on any platform Cargo supports; macOS is supported only that way. Not offered: a macOS archive, an installer, signed Windows executables. |
| P7 | Sanctioned copies | The texts allowed to repeat a fact, and the contract section each copy names. | `packaging/player-README.md` and `packaging/author-README.md`, each naming the contract section it copies; the controls table in `README.md`, naming its contract section. |
| P8 | Private references | Repositories, paths, URLs, tracker reference patterns and internal process vocabulary that must not appear in public texts. | Issue and PR numbers, in the `#`-prefixed form a tracker writes. Process vocabulary: ticket, spec number, scope number, and the internal tool and agent names. The architecture page's "contract map" is a product term. |
| P9 | Placeholder tokens | Each allowed token, and the texts where it may appear. Retired tokens that must not appear. | `@@VERSION@@` only in the two archive READMEs; every other token a text uses is filled before that text is published. Retired: `@@WIKI_URL@@`. Archive READMEs link contracts at the release tag: `<repository URL>/blob/v@@VERSION@@/…`. |
| P10 | Style choices | Spelling variant, serial comma, contractions, em dash limit, image policy, code fence tags, the platform order for commands and whether multi-page guides end each page with a "Next" pointer. | American spelling; no serial comma; no contractions; at most one em dash pair per paragraph; no images in the Guide; fence tags `console`, `powershell`, `sh`, `ron` (pack data), `unmei` (scenes) and `mermaid` (diagrams); PowerShell first, then `sh`; each Guide page ends with a "Next" pointer and a relative link back to the Guide home. |

When P10 leaves a choice unset, use these defaults:

- American spelling;
- no serial comma;
- no contractions;
- at most one em dash pair per paragraph;
- images allowed, with alt text;
- a "Next" pointer on every page of a multi-page guide.

## 3. Already machine-checked

Do not spend review effort on what the P3 checks cover.

A link check proves that a link resolves, not that it points at the right section. Rule B1 covers the second part.

## 4. Sources of authority

The sources are ranked in order of precedence. When two sources disagree, the higher one wins.

1. **The contracts** (P4): the glossary, the specs and the decision records. They own every behavioral fact. The glossary owns the one name for each concept and the words to avoid for it. When a project has no specs or decision records, the reference page or code that P4 names owns the behavioral fact instead. In § 6, "contract" means that owner, and a link to a contract section means a link to that page section or source file.
2. **The text's assigned job** (P1): which page does which job, and what it must contain.
3. **This page:** how the text reads.

## 5. The Cooperative Principle

The rules in § 6 apply H. P. Grice's Cooperative Principle to written docs: "Make your contribution such as is required, at the stage at which it occurs, by the accepted purpose or direction of the talk exchange in which you are engaged." [S1]

Here, the "talk exchange" is a stranger using a page for the one job P1 gives it: following steps, looking up a fact or reading an explanation through. A reader cannot ask a follow-up question, so the text must answer each question before the reader needs to ask it. The guidance this page draws on is listed in § 9.

**Every sentence earns its place.** Each sentence contributes a fact, an instruction, a qualification, an example or a necessary transition. A sentence that only makes the text sound polished, important or complete is cut.

| Maxim | Grice's wording [S1] | Question for the reviewer | Rules |
| --- | --- | --- | --- |
| Quantity 1 | "Make your contribution as informative as is required (for the current purposes of the exchange)." | Can the stranger finish each step with only this page and the pages it links? | A2, A4, A5, A10, B4, F2, F4, F11 |
| Quantity 2 | "Do not make your contribution more informative than is required." | Does anything here not help the reader with this page's task? | A1, A6, A7, A8, A9, B1, B2, D4, D13, F13 |
| Quality | "Try to make your contribution one that is true." Do not say what you believe is false, or that for which you lack adequate evidence. | Is every claim true of the current build and backed by a contract or a run? This includes what a sentence takes for granted, not only what it states. | A10, B3, B5, B6, B7, B8, B9, B10, C5, D16 |
| Relation | "Be relevant." | Does this belong on this page, at this point, for this page's audience? | A1, A3, A6, A9, B2 |
| Manner | "Be perspicuous": "Avoid obscurity of expression", "Avoid ambiguity", "Be brief", "Be orderly". | Could the reader misread it, stumble on a word, or meet a step in the wrong order? | Obscurity: A4, C4, D5, D11, D14, F14. Ambiguity: C1, C2, D6, D10, D12, D15, D16, F5. Brevity: A7, D1, D4, D13. Order: A3, A5, D9, F2, F11, F12 |

Rules missing from this table still apply. The table points each maxim at its main rules only.

### When the maxims conflict

- **Truth first.** Quality wins every conflict. A shorter or simpler sentence that is no longer true is a Blocker.
- **Completeness beats brevity.** When a step needs a prerequisite or an expected result, keep it, even if it costs words. Grice's "for the current purposes of the exchange" sets the bar: the stranger's task decides what is required, not the word count. A cut that removes what establishes a presupposition in a later step is a Blocker (A10).
- **Ownership beats repetition.** When completeness pulls toward restating a contract, link the owning section at the point of need and add a one-clause gist (B1). Do not copy the rule.
- **Precision beats hedging.** When truth pulls toward "may" or "usually", name the condition instead (B8).

### No flouting

In conversation, a speaker may break a maxim on purpose so the listener infers a hidden meaning: irony, understatement, a joke. A stranger reading in a second language will take such a sentence literally. Never flout a maxim in these texts (D5).

### What the text implies

Readers infer things from what a text leaves out. These inferences are where a technically true page misleads, so the rules close each one. Several are scalar implicatures: a weaker word implies that a stronger one is false, as "some tests pass" implies that not all do. [S18] The same pattern makes "supports Windows" read as full support, and "you can" read as optional.

| Readers infer | Unless the text | Rule |
| --- | --- | --- |
| A capability the README does not mention does not exist. | routes to it, or links the page that covers it | A1 |
| "Restart", "again" or "your API key" means the reader already has that state or thing. | establishes it in a prerequisite or an earlier step | A10 |
| A platform not named is unsupported, and one named is fully supported. | names the support exactly, including what is not offered | B4 |
| "Simply" or "just" means the step is trivial, so failing at it is the reader's fault. | drops difficulty judgments | D4 |
| "You can" means the step is optional. | uses "you can" only for optional steps | D10 |
| "You may want to", "consider" or "it is recommended" means the step is optional. | states whether the step is required, optional or advised, and warns before a risk | D15 |
| "New" or "the latest version" means the version the reader has. | names the version or date | D16 |
| "You entered an invalid key" means the reader caused the problem. | states what went wrong and the fix | D17 |
| A step without a warning is safe to repeat and changes nothing else. | warns before a step that overwrites, deletes, costs money or sends data over the network | F11 |
| Numbered steps must run in that order. | says so when the order does not matter | F12 |

## 6. Rules

Every rule is a judgment, not a word ban. A listed word is a prompt to look closer. It is a finding only when it causes the defect the rule names. A finding names a defect in the text, never a guess that a model wrote it. The style rules in § 6 E keep a project's texts consistent. They are not signs of authorship, and a finding under them cites the rule, not the writer.

Each rule has an ID and a fixed severity:

- **Blocker:** a wrong fact, a private leak or a step a stranger cannot complete.
- **Major:** a restated contract, a vocabulary violation or an instruction that has two readings.
- **Minor:** style.

### A. Audience and purpose

- **A1 (Major).** Each page does the one job P1 gives it, for the one audience P1 names. Content that belongs to another page or serves another audience is a link to that page. Example: CI details and design rationale in an install guide belong in contributor or design docs. [S13]
- **A2 (Blocker).** The reader is a stranger, as P2 describes each audience. Each step names everything the reader needs to complete it. Nothing assumes the repository, the maintainer or a previous page the reader may have skipped, unless the page links it as a prerequisite.
- **A3 (Minor).** The first paragraph says what the page gets the reader, in one or two sentences. [S6]
- **A4 (Major).** A term the reader may not know is explained at first use, or links its glossary entry or owning contract section.
- **A5 (Blocker).** A procedure lists its prerequisites before step 1. These are what to download or install, which page to finish first and which files or folders must exist. A missing prerequisite is a step the stranger cannot complete. [S6] [S11]
- **A6 (Major).** No background that the page's task does not need: history, design rationale, alternatives. Link the page that owns it. Say each point once, at the place where it applies, not in an introduction and again in the step. [S13]
- **A7 (Minor).** Start with the task, not with atmosphere. End with the next step or the "Next" pointer (F10), not with a summary or a closing paragraph about why the topic matters.
- **A8 (Major).** Write for the reader who arrives after the work is done. Describe the lasting behavior and scope. Leave out the drafting history:
  - a reply to a review comment;
  - "as requested";
  - a note about what an earlier draft said;
  - an instruction addressed to the agent that edits the page.

  When an omission has a lasting reason, state that reason as scope: "Database selection is outside this page's scope."
- **A9 (Major).** Describe a feature by what the reader gets or must do, not by how it is built. Keep a mechanism only when it changes what the reader does or expects, and say it in the reader's terms. Example: write "Finds your PC automatically, with no network setup", not "Discovers the host through the Steam Link stream". Write "Holds your gaze while you blink", not "Applies a One Euro filter and a deadzone". The implementation belongs in developer docs (A1).
- **A10 (Blocker).** Each presupposition in a step is true for every reader in the page's P2 audience, or a prerequisite or an earlier step establishes it. A presupposition is a fact a sentence takes for granted instead of stating. It survives negation: "Do not restart the gateway" still assumes the gateway is running. [S9] Words that commonly carry one:
  - "the" before a noun: "the config file" assumes exactly one exists;
  - change of state: start, stop, restart, resume, continue;
  - repetition: again, still, another;
  - possessives: "your API key" assumes the reader has one;
  - time clauses: "after you install", "before you run", "since you enabled";
  - knowledge verbs: know, notice, realize.

  A presupposition that is true for the whole audience is fine and needs no extra words. Report one only when it is false for some reader the page serves. Example: "Restart the gateway" fails for a reader who never started it. Write "Start the gateway", or add the step that starts it.

### B. Facts and ownership

- **B1 (Major).** A behavioral fact links the contract section that owns it instead of restating it. Behavioral facts are controls, file and credential paths, commands and flags, environment variables, platform requirements and limits. The link lands on the section that states the fact, not on the top of the file. A one-clause gist next to the link is fine. A copied rule, list or table is not.
- **B2 (Major).** A fact is repeated only in a sanctioned copy (P7). Each copy names its contract section.
- **B3 (Blocker).** Every fact that remains matches the current contract and build. Each command that a change adds or alters was run once against the current build, and its shown output matches. When a change alters the behavior a page's commands show, every command on that page is run again. The change record (pull request or commit message) lists those commands.
- **B4 (Blocker).** Platforms and install methods match P6 exactly. No text implies another platform, an installer or a signed build. Where a stranger would look for something that is not offered, say that it is not offered.
- **B5 (Major).** No promise beyond the contracts. Compatibility wording uses the project's versioning promise as its contract states it. No text describes a feature that has not landed. [S4]
- **B6 (Major).** Component, package and binary names match the page that P5 names as their owner. Other pages link it instead of restating them, except for a one-line summary in the project README.
- **B7 (Major).** No judgment without evidence. Replace it with the fact, behavior or limit that supports it, such as a contract guarantee, or delete it. Words to inspect:
  - strength: powerful, fast, robust, seamless, lightweight, secure, intuitive, scalable;
  - significance: pivotal, transformative, groundbreaking, crucial, vital, important to note;
  - benefit: empower, leverage, unlock, elevate, enhance, streamline.

  Keep a word that is the established term for a defined property, and link its definition.
- **B8 (Major).** No bare hedge: may, might, usually, should work, in most cases. Name the condition under which the thing happens, or link the contract section that defines it. Keep "may" only when the contract itself leaves the outcome open, and say so. Give one concrete boundary instead of several general caveats.
- **B9 (Blocker).** No invented specifics. Do not add a number, a file name, a command output or a reason that no contract, source or run supports. This applies most when making a vague sentence concrete. In an evidence record, label each statement as observed, inferred or unknown, and state the basis of each inference.
- **B10 (Major).** A reason comes from its owner. The reason for a design choice, a scope limit or a change comes from a contract, a decision record or the maintainer. Infer it from neither the code nor the diff. With no recorded reason, state the behavior and leave the reason out, or ask.

### C. Vocabulary

- **C1 (Major).** Use the glossary term for each concept, and none of its avoided words in that sense. Example: if the glossary says **save slot** and lists "save file" to avoid, write "save slot". An avoided word is fine in an unrelated sense. Report a hit only when the word names the concept. [S11]
- **C2 (Major).** One name per concept within a page and across a multi-page guide. Do not switch between synonyms for variety. [S11]
- **C3 (Minor).** Name the product as P5 writes it. Write binaries and packages in code format, exactly as they are named.
- **C4 (Minor).** Put commands, flags, file and directory names, environment variables, config keys and code identifiers in code format. Do not put ordinary words in code format for emphasis. [S11]
- **C5 (Major).** Quote a UI label exactly as the app shows it and as its contract spells it, with the same capitalization.

### D. Plain English

- **D1 (Minor).** Short sentences with one idea each. Split any sentence over about 25 words, or any sentence that needs a semicolon to hold together. [S11]
- **D2 (Minor).** Address the reader as "you". Write steps in the imperative ("Copy the folder"). Use the active voice: say who or what acts. [S6] [S11]
- **D3 (Minor).** Use the present tense for behavior: "the CLI validates", not "the CLI will validate". [S11]
- **D4 (Minor).** No filler or judgment of difficulty: simply, just, easy, easily, obviously, of course, basically, please, note that. [S3]
- **D5 (Major).** No idioms, jokes or culture-bound phrases. Many readers are not native English speakers. [S5] [S11]
- **D6 (Major).** No pronoun or "the" phrase with two possible referents. When "it", "this" or "the config file" could mean two things, name the thing: "the user config at `~/.tool/config.toml`". [S11]
- **D7 (Minor).** Say what to do, not only what to avoid. Pair each "do not" with the alternative.
- **D8 (Minor).** Use digits for versions, sizes, measurements, limits and step numbers: "up to 20 lines", "at most 3 attempts", "step 4". Spell out a small count in running prose: "two subcommands", "three files". [S11]
- **D9 (Minor).** Put the condition or goal before the instruction: "To validate the config, run …", "On Windows, …". The reader can then skip an instruction that does not apply. [S2] This follows the given-before-new principle: start with what the reader already knows, such as their goal or situation, and end with the new information, the instruction. [S20]
- **D10 (Major).** Use "you can" only for an optional step. Write a required step as an imperative, and a hard requirement with "must". [S11]
- **D11 (Minor).** When a page keeps a mechanism (A9), name it, not a metaphor: "the CLI checks every file before it writes", not "the CLI is the project's gatekeeper".
- **D12 (Minor).** Use "not X but Y" only when the reader could mistake one for the other: "validation fails at load, not at runtime". Do not set up a contrast that nobody holds.
- **D13 (Minor).** List only the items that exist. Do not round a list up to three for rhythm, and do not write "from X to Y" for items that form no range.
- **D14 (Minor).** Use verbs, not nominalizations: "the CLI validates the file", not "validation of the file is performed". [S11]
- **D15 (Major).** Each instruction states its force directly, so a reader who takes the text literally can tell what to do. [S10] Use:
  - an imperative, or "must", for a required step: "Back up the folder";
  - "optionally" or "you can" for an optional step (D10);
  - "we recommend" with its condition for advice: "On a shared machine, we recommend a separate user";
  - a warning before the step for a risk (F11): "This step deletes the folder. Back it up first."

  Do not phrase an instruction as a statement, a question or a polite suggestion: "You may want to back up first", "It is recommended to use a proxy", "Consider enabling sync", "Want faster builds?". Use each modal verb in one sense only. "Must" states a requirement, never a conclusion ("the gateway must be running, since the port is in use"). "Should" does not state a requirement. Judge each hit by what a literal reader would take it to mean, not by what the writer meant.
- **D16 (Major).** Anchor each word that depends on when, where or by whom it is read to something that stays fixed. A reader may see the sentence later, out of order, on another screen or alone in a search result. [S17]
  - Time: do not describe behavior as "new", "now", "currently", "latest" or "recently". Name the version or date: "added in 2.3". Time-stamped texts that P1 lists, such as release notes and changelogs, may use these words. [S11]
  - Position in the text: do not write "above", "below" or "the previous section". Link the target by name (F6), or write "earlier" or "following" with that link. A lead-in directly before its list or steps, such as "the following steps:", needs no link. [S11]
  - Position on screen: do not locate a control by "left", "right", "top" or "bottom". Name its UI label (C5) and the group or menu that holds it. [S11]
  - Person: use "we" only for the project as the author, as in "we recommend" (D15). Do not use "we" for the writer and reader together, as in "we now configure the proxy"; address the reader as "you" (D2). [S11]

  "You" needs no anchor. Report a hit only when the anchor is missing where the sentence can be read.
- **D17 (Minor).** Describe a problem by what went wrong, not by who caused it, and pair it with the fix. [S21] Write "The key is not valid. Copy it again from Settings → API keys.", not "You entered an invalid key." Write "Enter a name.", not "You did not enter a name." A sentence that names the reader as the cause adds no fact they need and works against them, which politeness theory calls a threat to the reader's face. [S22] State the fix instead of an apology such as "sorry". [S21]

Plain English never means a vaguer word. Keep the precise term, such as a glossary term or a CLI flag, and explain it at first use (A4).

### E. Spelling and punctuation

Each rule applies the P10 choice, or the § 2 default when P10 leaves it unset.

These choices suit documentation, which uses a formal written register. A register is the variety of language a situation calls for, and registers differ in which features they use often. [S15] A text in a conversational register, such as a community post or an announcement, uses fewer colons, em dashes, headings and bold, because those marks read as formal there. When P1 includes such a text, record its punctuation choices in P10 as a separate entry.

- **E1 (Minor).** One spelling variant across all texts in scope. Do not rewrite existing contracts for spelling. [S11]
- **E2 (Minor).** Follow the serial comma choice. When a list would otherwise read wrongly, add or drop the comma to fix the reading.
- **E3 (Minor).** Keep em dashes within the limit. Prefer a colon, parentheses, a comma or a new sentence. The bold-term-then-explanation pattern in a list uses a colon. [S11]
- **E4 (Minor).** Use sentence case in headings. Capitalize only the first word, proper nouns and glossary terms that the product capitalizes. [S11]
- **E5 (Minor).** Follow the contraction choice, and be consistent within a page.

### F. Structure and formatting

- **F1 (Minor).** One H1, matching the front-matter title when the project uses front matter. A summary, where present, is one sentence that says what the reader can do after reading.
- **F2 (Major).** A procedure is a numbered list or numbered headings, with one action per step. When the reader needs to confirm a step worked, show the expected result. [S11]
- **F3 (Minor).** Tag each code fence with its language:
  - `console` for a command with its output, where commands start with `$ `;
  - the shell's own tag (`sh`, `powershell` and so on) for commands to copy;
  - the file's language for file contents, using the tags P10 lists.
- **F4 (Major).** When a command differs between platforms, show each version, in the P10 platform order. When it is the same on all platforms, show it once.
- **F5 (Minor).** Write placeholders exactly as the tool's `--help` writes them, for example `<file>`, and use them the same way across pages. Use `path/to/dir` only for a path the reader chooses. [S11]
- **F6 (Minor).** Link text names the target and, for a section, the section: "`docs/config.md` § 4 Provider configuration". Never write "here" or "this link". [S11]
- **F7 (Major).** Links into the tree are relative. Absolute links into the project go only through a P9 token.
- **F8 (Major).** Images follow the P10 image policy. Every image has alt text that states what the image shows. [S11]
- **F9 (Minor).** Match the structure to how P1 says the reader reads: numbered steps for following, tables or lists for looking up and prose for reading through. Use a table only for lookup data, such as controls or a contract map. Do not put an explanation inside a table cell. Use prose or a list for explanations. Add a heading, bullets or bold only when they help the reader find or compare something. [S11] [S13]
- **F10 (Major).** When P10 calls for it, each page of a multi-page guide ends with a "Next" pointer. The page also ends with a relative link back to the guide's home page.
- **F11 (Major).** A warning comes before the step it guards, in that step. A step that overwrites a file, deletes data, costs money or sends data to a network service says so before the reader acts.
- **F12 (Minor).** Numbered steps tell the reader to follow that order. When the order does not matter, use bullets or say "in any order". [S11]
- **F13 (Minor).** Each link helps the reader finish the task or check a claim. Do not add a list of related links for completeness.
- **F14 (Minor).** Each table has a header row. Do not carry meaning by color alone: state it in text as well. [S11] [S12]

### G. Public-export hygiene

These rules apply to every text that P1 marks public.

- **G1 (Blocker).** No reference to anything in P8: private repository URLs, internal tracker numbers or private paths.
- **G2 (Blocker).** No internal process vocabulary from P8, such as internal tool names, agent names or ticket numbers. A product term that contains one of these words is not process vocabulary. A contributor-facing text may describe the public contribution process.
- **G3 (Blocker).** Only the P9 tokens appear, each only in the texts P9 allows. No retired token appears.
- **G4 (Blocker).** No real credential, and nothing that looks like one. An example key is a truncated placeholder such as `sk-...`. A troubleshooting step never asks the reader to attach a credential file.
- **G5 (Major).** A text shipped inside a release links contracts at that release's tag, never at a moving branch such as `main`.

## 7. Review procedure

1. Run the P3 checks and record their results.
2. Read the target's assigned job, audience and reading mode (P1). Then read the target once as the stranger in A2, in that reading mode. Note every point where you would get stuck and every passage written for a different audience.
3. **Maxim pass.** For each section of the target, ask the five reviewer questions in § 5. Then check the target against each row of the implication table. A problem that no rule in § 6 names is still a finding: report it under the maxim's name.
4. Apply the rules in order A, B, C, D, E, F, G. For B1 and B3, open each linked contract section and compare. Steps 5 to 7 add searches for some rules; run each search when you reach its rule.
5. For C1, scan the target for the avoided words of every glossary entry, and judge each hit in context.
6. For the rules below, search the target and judge each hit in context. A hit is a finding only when it causes the defect the rule names (§ 6).
   - A9: read each feature list and capability description, and check that each item states what the reader gets.
   - A10: search each procedure for the trigger words A10 lists, and check each hit against the page's audience and the steps before it.
   - B7, B8 and D4: search for the words each rule lists.
   - D10: search for "you can".
   - D12 and D13: search for "not just", "not only", "ranging from" and "from … to".
   - D15: search for "you may want", "you might want", "consider", "recommended", "should", "must" and question marks inside procedures, and judge each hit as a literal reader would.
   - D16: search for "new", "now", "currently", "latest", "recently", "above", "below", "previous", "left", "right", "top", "bottom", "we", "our" and "us", and check that each hit has an anchor wherever the sentence can be read.
   - D17: search error and troubleshooting text for "you entered", "you did not", "you didn't", "you forgot", "you failed" and "sorry".
7. For G1 to G3, search the target for:
   - every P8 entry;
   - issue-number patterns such as `#[0-9]`;
   - the token marker the project uses;
   - moving branch names in links.

## 8. Finding format

One finding per line, most severe first:

```text
<rule ID, maxim or Authority> <severity> <PLAUSIBLE|CONFIRMED> <file>:<line> "<quoted text>" → <proposed fix or question>
```

- **Label:** a rule ID from § 6. Use a maxim name for a problem no rule names (§ 7 step 3). Use Authority, with severity Major, when a rule on this page conflicts with a project contract; quote this page's rule and name the contract section in the fix field.
- **Confidence:** PLAUSIBLE when the finding depends on a fact you could not verify in the source, CONFIRMED otherwise.
- **One sentence, several rules:** report one finding under the most severe rule, or the first in § 6 order when severities are equal. Name the other rule IDs in the fix field.

For example:

```text
C1 Major CONFIRMED docs/guide/getting-started.md:12 "open the save file" → "open the save slot"
Quantity Blocker CONFIRMED docs/guide/configure.md:20 "Enter your key in Settings." → name the Settings group and row (C5), and link the contract section that owns them
```

A proposed fix must keep every fact, condition, exception and qualification of the original. If the fix needs a fact you do not have, write a question instead of inventing it (B9). Propose the smallest change that removes the defect. A fix that shortens a text must not remove what establishes a presupposition in a later step (A10). Keep the author's terms and structure, and leave a wider rewrite to the author.

End the report with the P3 check results and any rule you could not apply, with the reason (for example, an empty setting in § 2).

## 9. Sources

[S1] to [S8] were read or written on 2026-11-06 unless their entry says otherwise. Later entries give their own dates. The rules are this page's own application of these sources. Neither the Google nor the Microsoft style guide cites Grice.

A source tag means the source supports the rule's main point. Where a rule goes beyond its source or differs from it, the source entry says so. A rule with no tag rests on [S7], [S8] or the reasoning listed in [S16]. Examples throughout this page are reasoning, not quotations.

- **[S1]** Wikipedia, "Cooperative principle", https://en.wikipedia.org/wiki/Cooperative_principle. This is the source of the quoted wording of the principle and the maxims. It cites Grice, "Logic and Conversation" (1975), in *Syntax and Semantics* vol. 3, p. 45. The 1975 text itself was not read, so the wording is secondary. A short form appears in the Stanford Encyclopedia of Philosophy, "Implicature", https://plato.stanford.edu/entries/implicature/.
- **[S2]** Google developer documentation style guide, "Sentence structure", https://developers.google.com/style/sentence-structure: "try to mention the circumstance, conditions, or goal before you provide the instruction."
- **[S3]** Google developer documentation style guide, "Tone", https://developers.google.com/style/tone: avoid "simply, It's that simple, It's easy, or quickly in a procedure." This supports D4.
- **[S4]** Google developer documentation style guide, "Future features", https://developers.google.com/style/future: "Avoid documenting future features or products, even in innocuous ways." This supports B5.
- **[S5]** Microsoft Writing Style Guide, "Global communications", https://learn.microsoft.com/style-guide/global-communications/: "It's usually safe to assume your content will be read in many countries and by readers whose primary language isn't English." This supports D5. The page has no idiom rule of its own.
- **[S6]** digital.gov, "Plain language principles", https://digital.gov/guides/plain-language/principles: "write for your audience", "Use the active voice", "Organize the information. Prepare readers for what to expect." This supports A3, D2 and A5.
- **[S7]** Provenance note, not a checkable source: several rules come from "Language Guidance for Clear, Substantive LLM-Assisted Writing", version 1.0, reviewed 2026-11-06, which is unpublished. Those rules carry no citation tag because they rest on their editorial merit alone. The studies that guidance cites were not checked here. The rules are:
  - § 5 "Every sentence earns its place";
  - the § 6 note on authorship and the § 6 D note on plain English;
  - A7, A8, B7, B9, B10, D11, D12, D13, F9 and F13;
  - the § 8 smallest-fix rule.
- **[S8]** Provenance note, not a checkable source: changes made on 2026-11-09 from a review conversation about verbose, model-written docs. They rest on their editorial merit alone. The examples in A1 and A9 paraphrase public cases from that conversation: a project install guide that mixed user steps with CI and design detail, and a README feature list that described implementation instead of outcomes. The changes are:
  - the audience and reading-mode columns in P1, and the § 5 reading of the "talk exchange";
  - the audience clause in A1, the new rule A9 and the A9 scope clause in D11;
  - the register note in § 6 E;
  - the reading-mode and table-cell clauses in F9;
  - the audience check in § 7 step 2 and the A9 check in § 7 step 6.
- **[S9]** Stanford Encyclopedia of Philosophy, "Presupposition", https://plato.stanford.edu/entries/presupposition/, read on 2026-11-09 (the first 100,000 characters). It defines a presupposition as information a speaker takes for granted rather than asserts, and gives negation as the standard test (§ 1.2, § 1.3). Its trigger list (§ 1.1) includes definite descriptions, aspectual verbs such as "stop" and "continue", temporal clauses with "before", "after" and "since", and factive verbs. "Again", "still", "another" and possessives are not in that list; A10 includes them on this page's own judgment. This supports A10, the A10 row of the § 5 implication table and the "the" clause in D6.
- **[S10]** Stanford Encyclopedia of Philosophy, "Speech Acts", https://plato.stanford.edu/entries/speech-acts/, read on 2026-11-09 (the first 100,000 characters). It describes indirect speech acts, where one act is performed by way of another, such as a request made as the question "Can you pass the salt?" (§ 3.4). It states that grammatical mood does not by itself determine an utterance's force (§ 4), and that a speech act can misfire without the addressee's uptake (§ 2.2). The "literal reader" test in D15 is this page's application of uptake to readers who cannot ask the writer. The modal-verb clause in D15 is this page's own judgment. This supports D15 and the D15 row of the § 5 implication table.
- **[S11]** Google developer documentation style guide, read on 2026-11-09. Each page below supports the rules named after it:
  - "Highlights", https://developers.google.com/style/highlights: second person and active voice (D2), American spelling (E1), sentence case (E4), numbered lists for sequences and bulleted lists for other lists (F2, F12), descriptive link text (F6) and alt text (F8). Google also recommends serial commas; this page's § 2 default omits them.
  - "Tense", https://developers.google.com/style/tense: present tense for general behavior, and "will" only for an action that happens later (D3).
  - "Procedures", https://developers.google.com/style/procedures: numbered steps, one action per step, the action before its result (F2), making sure readers have what they need beforehand (A5) and marking optional steps with "Optional:" (D10). The page does not address steps that can run in any order, so that part of F12 is reasoning.
  - "Write for a global audience", https://developers.google.com/style/translation: short sentences (D1), one term per concept (C1, C2), no idioms, slang or humor (D5), a noun in place of an unclear pronoun (D6) and active verbs over noun phrases (D14). The 25-word threshold in D1 is this page's own.
  - "Placeholders", https://developers.google.com/style/placeholders: consistent placeholder names, explained at first use (F5). Google writes placeholders in uppercase with underscores. F5 instead follows the tool's `--help`, so the reader matches what the tool prints.
  - "Notices", https://developers.google.com/style/notices: a warning marks a step that may be irreversible, such as one that loses data. The page does not say where a warning goes, so the placement in F11 is reasoning.
  - "Cross-references and linking", https://developers.google.com/style/link-text: link text that makes sense without the text around it, never "click here" (F6).
  - "Contractions", https://developers.google.com/style/contractions: recommends common contractions. This page's § 2 default omits them. No rule cites this page.
  - "Dashes", https://developers.google.com/style/dashes: a colon or a period separates an item from its description (E3). The page sets no em dash limit, so the limit is this page's own.
  - "Numbers", https://developers.google.com/style/numbers: numerals for versions, technical quantities, limits and step numbers, and words for zero to nine elsewhere (D8).
  - "Tables", https://developers.google.com/style/tables: tables only for two-dimensional data (F9) and a marked header row (F14). Google allows long text in a cell; the F9 ban on explanations in cells is this page's own.
  - "Code in text", https://developers.google.com/style/code-in-text: code font for commands, file names, paths, environment variables and code identifiers (C4). The page does not address code font for emphasis, so that clause is reasoning.
  - "Capitalization", https://developers.google.com/style/capitalization: sentence case for titles and headings (E4).
  - "Timeless documentation", https://developers.google.com/style/timeless-documentation: avoid "new", "now", "currently" and "latest" in product docs because they go stale, and add a date or version where one is needed. The page allows them in time-stamped content such as release notes (D16). It does not list "recently"; D16 adds it on this page's own judgment.
  - "Accessibility", § "Document rendering", https://developers.google.com/style/accessibility: no directional language such as "above", "below" or "right-hand side" to orient the reader, and "earlier", "preceding" or "following" instead (D16).
  - "Second person", https://developers.google.com/style/person: "you" rather than "we", with "we" allowed for the organization as author when the reference is clear (D2, D16).
- **[S12]** W3C, Web Content Accessibility Guidelines 2.2, Success Criterion 1.4.1 "Use of Color" (Level A), https://www.w3.org/WAI/WCAG22/Understanding/use-of-color.html, read on 2026-11-09: color is not the only visual means of conveying information. This supports the color clause in F14.
- **[S13]** Diátaxis, https://diataxis.fr/ and "The map", https://diataxis.fr/map/, read on 2026-11-09. It names four kinds of documentation (tutorials, how-to guides, reference and explanation), each serving a different user need. Its "Blur" section warns that writing style and content drift into places where they do not belong. This supports A1, A6 and the reading-mode clause in F9. Diátaxis does not state a one-audience rule; the audience clause in A1 rests on [S8].
- **[S14]** Semantic Versioning 2.0.0, https://semver.org/, read on 2026-11-09: major for incompatible changes, minor for compatible additions, patch for compatible fixes. The specification applies to software that declares a public API. Treating this page's rules, § 2 and § 8 as that interface, and defining a major change by new findings, is this page's own application.
- **[S15]** Douglas Biber and Susan Conrad, *Register, Genre, and Style*, Cambridge University Press, 2009, chapter 8 "Multidimensional patterns of register variation". Only the chapter summary was read, at https://www.cambridge.org/core/books/abs/register-genre-and-style/multidimensional-patterns-of-register-variation/A6E08533A7633F2FCD782ADCD52A29A9, on 2026-11-09. It defines register features as features that occur more often in one register than in others, and states that conversation differs sharply from academic writing. This supports the register definition in § 6 E. The summary names no punctuation features, so the claim about colons, em dashes, headings and bold is reasoning.
- **[S16]** Reasoning, not a checkable source: no source was found for these parts, and they rest on this page's own judgment. The A10 and D15 extensions are listed in [S9] and [S10].
  - § 1 adoption order, the § 2 settings and defaults, § 4 precedence order, the § 6 severity levels and the § 8 finding format;
  - A2, A4, B1, B2, B3, B4, B6, B8, C3, C5, D7, E2, E5, F1, F3, F4, F7, F10 and the placement in F11;
  - the "any order" clause in F12 and the code-font emphasis clause in C4;
  - all of § 6 G, which follows from each project's own P8 and P9 settings.
- **[S17]** Stanford Encyclopedia of Philosophy, "Indexicals", https://plato.stanford.edu/entries/indexicals/, read on 2026-11-09 (the first 100,000 characters). It describes an indexical as an expression whose reference shifts from context to context, with examples including "I", "you", "we", "here", "there", "now", "today" and "this" (§ 1). The entry uses "indexical" and does not use the noun "deixis"; this page uses the linguistics term for the same class of words. The fixes in D16 come from [S11]. This supports the definition in D16 and the D16 row of the § 5 implication table.
- **[S18]** Stanford Encyclopedia of Philosophy, "Implicature", https://plato.stanford.edu/entries/implicature/, read on 2026-11-09 (the first 100,000 characters). It describes a quantity, or scalar, implicature as one where the speaker implicates the denial of a stronger proposition than the one said, with "some athletes smoke" implicating "not all athletes smoke" (§ 2, § 4). It describes Horn's scales, which order words such as "all", "most", "many" and "some" by strength (§ 15). This supports the scalar implicature note in § 5. The "supports Windows" and "you can" examples are this page's own application.
- **[S19]** Withdrawn in 5.1.1. No rule or note cites it.
- **[S20]** San José State University Writing Center, "Old Information before New Information", https://pdp.sjsu.edu/writingcenter/docs/handouts/Old%20Information%20before%20New%20Information.pdf, read on 2026-11-09: start a sentence with information the reader already knows and end with new information, so each new point connects to a known one. The handout cites no source. Wikipedia, "Information structure", https://en.wikipedia.org/wiki/Information_structure, read on the same day, defines given information as known from common knowledge or earlier in the text and new information as the rest, but does not state the ordering. This supports the given-before-new reason in D9.
- **[S21]** Google, "Technical Writing: Writing Helpful Error Messages", section "Set the right tone", https://developers.google.com/tech-writing/error-messages/set-tone, read on 2026-11-09. It says to describe what went wrong rather than who caused it ("The specified printer is offline", not "You specified a printer that's offline"), to tell users how to get it right rather than what they did wrong ("Enter a name"), to limit "sorry" and "please" because apology expectations vary by culture, and to avoid humor. This supports D17. The guidance is for error messages; applying it to troubleshooting text in docs is this page's own application.
- **[S22]** Wikipedia, "Politeness theory", https://en.wikipedia.org/wiki/Politeness_theory, read on 2026-11-09. It credits the theory to Penelope Brown and Stephen Levinson (1978, expanded 1987), building on Erving Goffman's concept of face. It defines positive face as the want to be approved of and negative face as the want to act unimpeded. It describes direct ("bald on record") phrasing as used where urgency, efficiency or the task comes first, and reports critiques that the theory is Western-centric. Brown and Levinson's book was not read, so this is a secondary source. This supports only the explanation in D17; the rule itself rests on [S21].

These were not verified and are not relied on:

- the 25-word sentence figure in D1, which is this page's own threshold;
- Joseph Devney, "Conversational Maxims and Technical Documentation", *STC Intercom*, whose page could not be read on 2026-11-06;
- the published critiques of Grice that the SEP entry lists.

## 10. Versions

This page uses Semantic Versioning, treating its rules, § 2 and the § 8 format as its public interface. [S14] The version is in the front matter and in the file name.

- **Major:** a page that passed review can now get a finding, or an adopter must change § 2 or a tool that reads the § 8 format.
- **Minor:** an addition that cannot produce a new finding, such as an example, a source or a clarification.
- **Patch:** a wording fix that changes no rule's meaning.

Versions:

- **7.0.0:** changes the § 8 format: a confidence field after the severity, one finding per sentence under its most severe rule, and the Authority label for contract conflicts. Severities are fixed.
- **6.1.0:** moves the versions to § 10, orders the implication table and the § 9 sources by ID, lists the § 7 searches by rule and adds clarifications for reviewers. No rule changes.
- **6.0.0:** adds D17 (problems stated without blame). [S21] [S22]
- **5.1.1:** removes the relevance test from § 5 and withdraws [S19], so reviewers apply one framework.
- **5.1.0:** adds the scalar implicature note in § 5, the relevance test under "Every sentence earns its place" and the given-before-new reason in D9. [S18] [S19] [S20]
- **5.0.0:** adds D16 (context-dependent words). [S17]
- **4.1.0:** adds sources [S11] to [S16] and labels the rules that rest on reasoning.
- **4.0.0:** adds D15 (instructions state their force). [S10]
- **3.0.0:** adds A10 (presuppositions) and the "the" clause in D6. [S9]
- **2.0.0:** adds audience and reading mode to P1, the audience clause in A1, A9 (outcomes before mechanisms) and the register note in § 6 E. [S8]
- **1.0.0:** first version.
