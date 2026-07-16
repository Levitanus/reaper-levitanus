# reaper-levitanus

![linux](https://github.com/Levitanus/reaper-levitanus/actions/workflows/build_linux.yml/badge.svg)
![windows](https://github.com/Levitanus/reaper-levitanus/actions/workflows/build_windows.yml/badge.svg)
![macos](https://github.com/Levitanus/reaper-levitanus/actions/workflows/build_macos.yml/badge.svg)

General purpose collections of actions and tools for [Cocos Reaper](reaper.fm).

Currently adds actions:
- normalize_all_takes_on_selected_items
- normalize_all_takes_on_selected_items (common gain)
- Take Pitch envelope snap *
    - Set snap for pitch envelope, or make it default, or turn off
- Set Take pitch envelope range
- ffmpeg gui
    - Opens dialog for setting up filters, settings and rendering project video with FFMPEG.
    - For now in early alpha stage.
    - Better to run Reaper from terminal while working with ffmpeg dialog:
        `RUST_LOG=error,reaper_levitanus=debug RUST_BACKTRACE=1 reaper`
    - It will produce a lot of helpful debug information.
- export OTIO timelines
- set OTIO FPS to median
- set OTIO FPS to Project FPS
- set OTIO FPS to first video in timeline
- export YouTube timecodes from markers

## OTIO export

The plugin can export REAPER timeline(s) to OpenTimelineIO `.otio` files.

### Actions

- `export OTIO timelines`
    - Exports OTIO timelines using current REAPER render settings and render targets.
- `set OTIO FPS to median`
    - Stores FPS policy in project ExtState. This is the default policy.
- `set OTIO FPS to Project FPS`
    - Stores FPS policy in project ExtState.
- `set OTIO FPS to first video in timeline`
    - Stores FPS policy in project ExtState.
- `export YouTube timecodes from markers`
    - Exports marker timecodes per render target and render bounds into `render_target_name.txt`.

### FPS policy

OTIO timestamps are written in frames (`RationalTime.value`) using selected FPS as `RationalTime.rate`.

- Median video (default): computes FPS for all source video files used in exported timeline and takes median.
- Project FPS: uses FPS from the first render target file (if available).
- First video: uses FPS from the first detected source video in exported timeline.

If FPS can not be detected, exporter falls back to `25.0`.

### Output files

For each render target, exporter writes OTIO file next to it:

- render target: `.../my_render.wav`
- otio output: `.../my_render.otio`

YouTube timecodes action writes a separate text file for each render target:

- render target: `.../my_render.wav`
- timecodes output: `.../my_render.txt`
