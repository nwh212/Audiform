# Judgement calls to review

The project lead asked (2026-10-28) that open questions be decided rather than waited on, and listed here for review. Each entry says what was decided and how to change it.

## Phase Curve window

- **Shown range is 20 Hz to 20 kHz** (was 10 Hz to 24 kHz), to match the EQ windows' ruler. Points set below 20 Hz or above 20 kHz still work; they sit at the graph's edge.
- **Frequency Scale (Linear or Logarithmic) is a view choice**, kept per computer like the EQ window's view, not saved in the session or undoable.

## Band EQ windows

- **Kept the response graph** above the new slider row, so the overall curve stays visible. Easy to drop if the window should be sliders only.
- **Range stays ±15 dB per band**, as before. A wider range (±24 or more) would only need the limit raised in `filters.rs`; old sessions would still load.
- **Added Alt+drag to draw across bands and a Flat button.** Alt+drag is a mouse gesture inside this window, not a new keyboard shortcut.

## Notch Filter window

- **One width for all six notches**, as the effect already had. A width per notch would be a new setting per notch (old sessions would load with the shared width); say if you want it.

## Center Channel Extractor

- **The Center Remover is retired, not converted.** Sessions that use it still play it exactly as before; it is just no longer in the menus. Converting old slots to the new effect would change how old sessions sound.
- **Discrimination defaults** (my picks, tune by ear): Crossover bleed 20%, Phase 15°, Amplitude 3 dB, Amplitude bandwidth 2 bins, Spectral decay 0%, FFT size 4096.
- **Frequency range presets** (my picks): Bass 20 to 250 Hz, Male voice 90 Hz to 10 kHz, Female voice 160 Hz to 12 kHz. The ranges are wide so a voice's consonants are caught too.
- **Center and Side go from -48 dB to +12 dB**; the bottom of the fader is silence ("off").
- **Remove Vocals preset** keeps the bass below about 150 Hz in the middle, as the old one did.
- **Imported .sesx sessions:** the extractor from other editors' session files is not brought in yet (its saved settings format is unknown to me); it is skipped with the usual notice.

## Convolution Reverb

- **The session stores the response file's path, not the sound itself.** Sessions stay small (a 10 second stereo response would add several MB to every save and undo step), but moving a session to another computer needs the response file too, as with the session's audio. The other way (embedding, like the Amp Model's cabinet) is possible if you prefer portability.
- **512-frame blocks** (about 11 ms at 48 kHz, made up for on playback and export). Smaller blocks would cost much more processing on long responses.
- **Defaults:** Wet -10 dB, Dry 0 dB, no pre-delay, full length.
- **No drag-and-drop of a file onto the window yet** (Load button only). The Files panel drags with its own pointer code, so this needs its own small piece of work.
- **Fixed during testing:** the window could reload the file in a loop when the core reported the path differently from how it was saved (slash direction on Windows). It now loads once per path the slot names.

## Time stretch

- **Used for playback too, not only for exports.** It is cheap enough, and playback and export stay identical (they always have been). If you would rather keep the old method live and the new one only for exports, that is a small switch.
- **The old method is gone,** not kept as an option. It was better than the new one only in one corner (very short frames on a dry single voice). Comparison files are in `review/stretch/` (not committed): the original, the old 1.5x and the new 1.5x.
- **Attack detection threshold** is my pick (a frame counts as an attack when its spectrum gains more than 30% and over 2.5 times its recent average). If drum hits still sound soft, it can go lower.

## README and screenshots

- **"Five themes" in the README** counts Dark, Prism, Moonlight, Aqua and Meadow (Sky Glass lives on its own branch).
- **Screenshots come from a mocked session** in a browser (no real audio): the waveforms are drawn from made-up shapes. The script is in my scratch folder; say if you want it kept in the repo (`scripts/`) to redo them later.
- **The banner image is still needed:** it never reached my disk. Commit it (for example to `brand/banner.png`) or attach it again and I will use it for the README header and the launch splash.

## Keyboard shortcuts

- **I built five shortcut features** that had keys assigned but nothing behind them, because they are everyday basics in a music editor: Nudge (Alt+comma/period), vertical zoom (Alt+=/-), Zoom Out Full (Shift+Backslash), Hide All Effect Windows (Ctrl+Shift+H) and Return to Start on Stop (Shift+X). These keys were already approved in CLAUDE.md, so no new keys were added.
- **Nudge distance is five pixels at the current zoom** (fine when zoomed in, coarse when zoomed out). A fixed time or grid step is the other common choice. Nudge moves audio clips only, not MIDI clips.
- **Pressing 9 (or 1) with no file open in the Waveform editor** opens the New Audio File dialog, which then takes the keys. That is how it was built; say if 9 should just switch to an empty Waveform editor instead.
- **Shift+D toggles the Waveform editor's spectrum view;** in the multitrack view it only flips that setting, so nothing visible changes there.

## Network prompts and the unsigned warning

- **One-time startup offer** to add the firewall rule (Windows, with plug-ins scanned, when no rule covers Audiform). Answering either way stops the offer; the Effects menu item stays.
- **MSI would not remove the "unsigned" warning:** an unsigned MSI gets the same SmartScreen warning. The fix is code signing (see the reply in chat for options). Until then: the installed app warns only once per download; the portable zip warns every start unless the zip is unblocked first (now in the README).

## Glass pointers

- **Only the I-beam and the resize arrows got glass versions.** The hand, crosshair and "not allowed" pointers keep their usual shapes; say if you want those too.
- **The pointer is swapped a frame after the mouse moves onto an element,** so the standard shape can flash for one frame. Not tested on Windows yet.

## Phosphor theme

- **Named Phosphor**, not after the machine you mentioned (naming rule).
- **Clips keep their track colors** (pink drums, orange bass and so on), since those are your session's own colors. A strict one-color screen would make every track green and hard to tell apart; say if you want clips forced to shades of green in this theme.
- **Record and errors stay orange-red** for the same reason.
- **The scan lines are faint on purpose.** They sit over everything, so stronger ones would hurt reading on long sessions.
