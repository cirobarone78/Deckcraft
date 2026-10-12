---
title: DeckCraft closes right after it starts
description: On Windows 11 the DeckCraft window appears and closes at once; start it with the Vulkan graphics backend instead.
order: 10
verified: 0.5.0
---

If DeckCraft flashes and quits, start it from PowerShell with `WGPU_BACKEND` set to `vulkan`.

## Symptoms

- The app shows up in Task Manager for a moment, or its window opens and closes at once.
- `deckcraft.log` ends with lines like `Exiting because of error: WGPU error: Parent device is lost`.

## Fix

1. Open the folder that contains `deckcraft.exe`. For the installer that is the `DeckCraft` folder inside Program Files. For the portable zip it is the folder you unzipped.
2. Open PowerShell in that folder.
3. Run these lines one at a time:

        $env:WGPU_BACKEND = "vulkan"
        & .\deckcraft.exe
        Remove-Item Env:WGPU_BACKEND

The last line resets the setting so later launches use the default again.
Do not set `WGPU_BACKEND` to `gl`. The README warns that it can bring the driver crash back on affected systems.

## Why this happens

By default the Windows app starts the graphics layer on DirectX 12 only. The README says that letting it also create an OpenGL instance can crash some graphics drivers (AMD's `atio6axx.dll`) before the window appears. A separate report on an NVIDIA PC (issue 153, still open for 0.5.0) shows the same symptom and says Vulkan works there.

## Find the log

The log is `%APPDATA%\DeckCraft\logs\deckcraft.log`. Attach it when you [report a problem](send-a-bug-report.md).

## Still stuck?

Ask in [Discord](https://discord.gg/artcraft) or add to [issue 153](https://github.com/storytold/deckcraft/issues/153) in the issue tracker.
