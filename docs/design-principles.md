# Design principles

Written down so the intent is on record. Altered Audio Editor, and Audiform after it, is its own product.

## Similar where it helps, ours everywhere else

- **Conventions we keep:** what a feature does and where an experienced editor expects to find it (multitrack and waveform editing, clip gain, fades, a mixer, a loudness tool). Functions and workflows are common ground in audio software.
- **Expression that is ours:** names, wording, icons, colors, panel layout, dialog structure and branding. We do not match another product pixel for pixel, and where another product's way is only cleaner, we take the idea and draw it our way.
- **Where we would rather do our own thing, we do.** The settings dialog only holds settings for things this app has or will have. Nothing is there just because another product has it.

## No AI unless you turn it on

- The app has no AI features today, and nothing leaves the computer (see PRIVACY.md).
- Any future AI tool is optional, off by default, and does nothing until the user turns it on and picks where it runs.
- Preferred order if we ever add it: tools running on the user's own machine first (local models, local text-to-speech or speech-to-speech), cloud services only when the user supplies their own key and switches them on. No account with us, no bundled keys, no hidden calls.
- This needs its own decision entry, a privacy review, and a change to the privacy check before any code is written.

## Words we use in public

- Say what the app is: free, private, no subscription, open source.
- Do not describe it as a clone of, a replacement for, or an alternative to another named product, and do not disparage anyone's product.
- The Help menu holds a support link and the book link only. The link is worded "support", not "donate".
