---
title: Imported transitions are too slow
description: In DeckCraft 0.5.0 a transition from a PowerPoint file can run longer than set in PowerPoint; set the Duration by hand.
order: 20
verified: 0.5.0
---

Select the slide, open the **Transitions** tab and type the time you want into **Duration:**.

## Symptoms

- You open a `.pptx` and its transitions run longer than in PowerPoint. A Morph set to 0.5 seconds in PowerPoint plays for 2 seconds.

## Fix

1. Select the slide.
2. Open the **Transitions** tab.
3. Set **Duration:** to the time you want, for example 0.5.
4. Repeat for each slide that needs it.

## Why this happens

PowerPoint leaves out the duration when it is 0.5 seconds, and DeckCraft 0.5.0 then uses the effect's own default instead: 2 seconds for Morph, 0.7 for Fade and 1 second for most others. This is issue 117. A fix is open as pull request 149 and is not in 0.5.0.

## Still stuck?

Ask in [Discord](https://discord.gg/artcraft) or add to [issue 117](https://github.com/storytold/deckcraft/issues/117).
