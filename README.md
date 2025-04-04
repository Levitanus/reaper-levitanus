# reaper-levitanus

![linux](https://github.com/Levitanus/reaper-levitanus/actions/workflows/build_linux.yml/badge.svg)
![windows](https://github.com/Levitanus/reaper-levitanus/actions/workflows/build_windows.yml/badge.svg)
![macos](https://github.com/Levitanus/reaper-levitanus/actions/workflows/build_macos.yml/badge.svg)

General purpose collections of actions and tools for [Cocos Reaper](reaper.fm).

Currently adds actions:
- normalize_all_takes_on_selected_items
- normalize_all_takes_on_selected_items (common gain)
- Take Pitch envelope snap *
    * Set snap for pitch envelope, or make it default, or turn off
- Set Take pitch envelope range
- ffmpeg gui
    * opens dialog for setting up filters , settings and rendering project video with FFMPEG. 
    * For now in early alpha stage.
    * the better to work with ffmpeg dialog, while running Reaper from command line \ terminal with a command: `RUST_LOG=error,reaper_levitanus=debug RUST_BACKTRACE=1 reaper`. It will produce a lot of helpful debug information.
