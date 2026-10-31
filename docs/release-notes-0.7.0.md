# Audiform 0.7.0

Windows is the main target. Linux and macOS (Apple silicon, unsigned) builds are experimental; if something is off there, please report it.

## New

- **ASIO that works with your interface.** Settings > Audio Devices picks one ASIO driver by name and loads only that one, so wrappers and virtual cables installed alongside can no longer get in the way. A buffer size picker (64 to 2048 frames, or the driver's own) sits beside it. Changing either while tracks are armed reopens their inputs at once.
- **Monitor Input with or without plug-ins.** The monitor button steps through off, on with plug-ins, and on without plug-ins (less delay). Monitoring goes through the track's effects, fader and pan, and follows changes to them. Plug-ins for monitoring start in the background, so recording never waits on them.
- **Insert Session as Clip** (Multitrack menu). Another session's whole mix lands on a track as one clip, so a heavy instrumental session costs the session you are working in no more than one audio clip. Save the inner session again and the clip updates itself. Right-click the clip to open its session or update it at once.
- **Stereo input meters.** An armed track on a stereo input shows left and right on their own bars.
- **Audio log.** `audio.log` in the app's config folder records the driver, every stream opened, how long Play took to start and why, how hard rendering works while playing, and every plug-in start and stop. Settings shows where it is. Attach it to problem reports.

## Faster

- **Dense sessions play on every core.** Tracks and buses render side by side, so a session with dozens of tracks and effects plays about three times faster than before on a four-core machine. Mixdowns and bounces speed up the same way.
- **Starting Play.** Plug-ins of every open session stay loaded when you switch between sessions, and any that need starting load side by side, so Play starts in under a second where it took 20 seconds and more.
- **Cheaper Reverb and Modulation**, with the same sound.
- **The editor** draws one playhead line over all lanes instead of repainting every lane, and only draws lanes that are on screen.
- **Opening sessions from the other editor** measures all their source files at once.
- **Clips open a second early,** so sessions with hundreds of clips never pause at a clip boundary.

## Changed

- **Turning an effect's dial while playing** no longer pauses or pops: the effect takes the new setting in place. EQs, filters and dynamics keep their state as they change.
- **Vox Strip** no longer has a Space module. Use the Reverb and Delay and Echo effects after it.

## Fixed

- The "effect update depth" error at startup or when opening a session, caused by the Audio Separation panel.
- Recording on ASIO at small buffer sizes could crash the first time; it now opens monitoring and playback the same way.
- macOS builds moved to a newer build machine.
