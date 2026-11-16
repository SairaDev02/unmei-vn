---
title: Live decisions with a NanoGPT key
summary: Add a NanoGPT key so a model answers a decision point, with what each request sends and costs, or play offline with no key.
keywords:
  - decisions
  - credentials
  - decision-point
  - settings-file
  - external-dependency
---
# Live decisions with a NanoGPT key

This page shows you how to add a NanoGPT key so that a model answers the demo's decision points,
and what each request sends and costs.

A pack can write an NPC's (non-player character's) reaction in advance. It can also leave the
reaction to a decision point, which asks a model what the NPC does, given the game state. A decision
point that a model answers is a **live decision**. Live decisions are optional: with no key, every decision point takes
the branch the pack author wrote as the fallback, and the demo plays end to end
([With no key](#with-no-key)).

Before you start, extract the player and check that it runs, as in
[`play-the-demo.md`](play-the-demo.md).

## Get a NanoGPT key

Unmei asks [NanoGPT](https://nano-gpt.com) for each decision, so the key is a NanoGPT key, billed to
your own NanoGPT account. Unmei has no account and no key of its own, and the player sends each
request to NanoGPT directly.

1. Sign in at [nano-gpt.com](https://nano-gpt.com), or create an account.
2. Adding funds costs your own money, and decisions spend them ([What it costs](#what-it-costs)). Add
   funds to the account.
3. Create an API key at [nano-gpt.com/api](https://nano-gpt.com/api) and copy it. It looks like
   `sk-nano-…`. Keep the copy for [Paste the key in Settings](#paste-the-key-in-settings).

A key that starts with `sk-or-` is an OpenRouter key. The player never sends it
([If you stored an OpenRouter key](#if-you-stored-an-openrouter-key)).

## What a live decision sends

With a key set, reaching a decision point makes one request, which carries:

- the decision model that the NPC pins: the demo pins `typesafe/jev-1.14` for Ace;
- the question's instructions and its criteria, one description for each action the answer can name;
- the state keys that the NPC's config lists, with their values: the demo's question reads
  `affinity_level` and `met_ace`.

An NPC with no pinned model sends nothing and takes the fallback.

The request goes to NanoGPT and nowhere else. NanoGPT does not store prompts by default and does not
train on them, and the upstream model provider keeps data under its own terms
([NanoGPT's privacy policy](https://nano-gpt.com/legal/privacy-policy)). The player sends no
moderation, data-retention or routing options, so the pinned model and the settings of your NanoGPT
account decide how the request is handled.

The model does not write text. It names one action, and the script continues at that action's
branch. The demo's question is Ace's greeting. Its actions are `warm` and `cold`, plus `none` for
when neither description fits, and `cold` is the fallback. If the model refuses the question, the
script also takes the fallback, and the session log says that the model refused.

How a pack declares these is
[`docs/content-format.md` § 4 Decision configs and decision points](../content-format.md#4-decision-configs-and-decision-points).
How the player sends the request is
[`docs/runtime-hardening.md` § Provider seam](../runtime-hardening.md#provider-seam).

## What it costs

NanoGPT bills each request to your own account, and it bills decisions on input tokens only. The
demo has a single decision point, so one playthrough makes one request, plus one each time you load a
save that is parked on it. Unmei charges no subscription and no fee, so the only money involved is
the funds on your NanoGPT account.

The price depends on the pinned model and on the size of the request. This page gives no amount. For
each decision that a model answers, the session log records the model and, when the response reports
one, the cost in US dollars as NanoGPT recorded the charge. The log is beside the pack folder
([`troubleshooting.md` § Where the log is](troubleshooting.md#where-the-log-is)).

## Paste the key in Settings

1. Open the Settings view. In play, press **Esc**, then choose **Settings**. On the title, choose
   **Settings**. Both open the same view.
2. Go to the **Decisions** group, the last group in the view.
3. Enter the key row: press **Enter** on it, or click the field. The row takes no typing until you
   enter it. With no key stored, its state line reads `Not set` and the field is empty.
4. Committing replaces any key already stored and sends the new key to NanoGPT to check it. Paste
   the key and press **Enter** to commit. The player writes the key to the credential file first,
   then checks it against NanoGPT. To leave the field and discard what you typed, press **Esc**.
5. Read the state line. `Checking…` means the check is still running, and the field takes no input
   until it answers. The check gives up after 5 seconds. Then the state line reads one of the
   following. In these lines, `sk-nano-…a1b2` shows the start of your key, and `a1b2` stands for its
   last four characters.

   - `Set — sk-nano-…a1b2`: NanoGPT accepted the key.
   - `Set — sk-nano-…a1b2 · not checked (see the log)`: the check could not reach NanoGPT, so the
     key is set but unverified. The session log says why.
   - `Invalid — does not look like a key`: the text cannot be a key, so nothing was written and no
     check ran. The field keeps your text. Correct it and press **Enter** again.
   - `Invalid — rejected by NanoGPT`: NanoGPT refused the key. It stays on disk, and play
     resolves decisions as it did before the commit. To replace it, paste another key.
   - `Invalid — an OpenRouter key; enter a NanoGPT key`: the text starts with `sk-or-`, so nothing
     was written and no check ran. The field is empty. Paste a NanoGPT key and press **Enter**
     again.

A key that shows `Set` applies in the same session, with no relaunch. To remove the key, choose
**Clear key**, then **Yes** in the confirmation. This deletes the stored key and returns play to the
offline fallback.

The key is stored for your user account only, never in the pack folder, so a pack can be shared
without carrying it. The group, the row's states, where the key is stored and how it is validated are
[`docs/title-and-menu.md` § Decisions settings](../title-and-menu.md#decisions-settings).

## The environment override

If the environment you launch the player from has a non-empty `NANOGPT_API_KEY`, that key wins
over any stored key. The state line then reads `From environment — sk-nano-…a1b2`, the field is not
editable and **Clear key** is disabled. To use a stored key again, unset the variable and relaunch.
The stored key is never overwritten while the variable is set. An empty or whitespace-only variable
counts as unset.

A variable that holds an OpenRouter key reads `From environment — Invalid, an OpenRouter key`, and
decisions take the fallback. To fix it, set the variable to a NanoGPT key, or unset it and relaunch.

## If you stored an OpenRouter key

If you stored an OpenRouter key with an earlier build, the key row reads
`Invalid — an OpenRouter key; enter a NanoGPT key`, and decisions take the fallback. The player
never sends an OpenRouter key to NanoGPT. Paste a NanoGPT key in the field, as in
[Paste the key in Settings](#paste-the-key-in-settings). The old key stays in the credential file
until the new key replaces it. The player does not convert or delete it on its own. To remove it
without a replacement, choose **Clear key**.

## With no key

The demo needs no setup to work. If no key is stored and the environment you launch from has no
non-empty `NANOGPT_API_KEY` ([The environment override](#the-environment-override)), every
decision point resolves to the pack's authored fallback, and the session log notes why. Offline is a
supported mode, not a fault
([`docs/runtime-hardening.md` § Runtime failure posture](../runtime-hardening.md#runtime-failure-posture)).
How a pack sets each fallback, and what the `none` action is, are in
[`docs/content-format.md` § 4](../content-format.md#4-decision-configs-and-decision-points).

## Next

- [`play-the-demo.md`](play-the-demo.md): the archive, the title screen and saving.
- [`troubleshooting.md`](troubleshooting.md): when a launch or a load fails.
- [`format-overview.md`](format-overview.md): where an NPC's decision config sits in a pack.

Guide home: [`README.md`](README.md), with the routes for players, authors and contributors.
