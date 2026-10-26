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
