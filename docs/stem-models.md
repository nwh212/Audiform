# Stem models: what Audio Separation offers and why

Audiform's Audio Separation panel downloads models on demand (see `src-tauri/src/separation/mod.rs` for the list, each with a pinned checksum). The engine runs the two band-split RoFormer families; models of other kinds cannot be added until the engine learns them. This page records the survey behind the list, so the next check (every major release, `docs/RELEASING.md` step 2) starts from here. Surveyed 2026-11-03.

## What ships

| Need | Model in the list | Why this one |
| --- | --- | --- |
| Vocals and instrumental | BS PolarFormer, Mel-Band Roformer (Kim), Mel-Band Roformer Deux | Best public scores on the common 100-song test, permissive or freely published. |
| Six stems at once (bass, drums, other, vocals, guitar, piano) | BS-RoFormer SW | Best public scores on every one of those stems. Its provenance is disputed and no licence is stated, which the list says. |
| Bass, drums, guitar, piano and keys on their own | X-LANCE fine-tunes | MIT licence; the same family as the six-stem model. |
| Guitar, trained for it | Mel-Band Roformer Guitar (chenCFD) | MIT licence, small (277 MB), a published score of 7.98 dB on the trainer's own test. |
| Lead vocal apart from backing vocals | BS-RoFormer Karaoke (frazer and becruily), and anvuew's | The two best public lead-vocal scores (10.10 and 10.22 dB) among single models. The first has no licence stated; anvuew's is GPL-3.0. Run the vocals model first, then one of these on the vocal stem, to get the backing vocals as the rest. |
| Bowed strings | BS-RoFormer Bowed Strings v2 (gilliaan) | The only public strings model, 4.69 dB on a public strings test. CC BY-NC-SA 4.0: free for non-commercial use, with credit, shared alike. |
| Noise | Mel-Band Roformer Denoise, and the aggressive one | As before. |
| Bleed between stems | Mel-Band Roformer Bleed Suppressor (unwa and 97chris) | Cleans what other stems left in an instrumental. No licence stated. |

## Looked at and left out, with the reason

- **Drum kit into kick, snare, toms, hi-hat and cymbals.** The best public model (an MDX23C by aufr33 and jarredou, 438 MB) is of a kind the engine does not run, its original release is gone and only mirrors remain, and no licence was ever stated. The permissive one (a Hybrid Demucs by inagoy, MIT) is also a kind the engine does not run and scores well below it. The best kit splitters of all are online-only services.
- **Synth lead, percussion.** The public models are re-uploads of unknown provenance or unverified links.
- **Everything at once (53 stems).** ZFTurbo's open 53-stem model (1.37 GB, MIT) wants a 16 GB graphics card and its per-stem quality is low. Useful as an instrument detector, not as a separator. Left out until the engine can run it in halves.
- **Speech, music and effects (film mixes).** Bandit v2 (Apache code, share-alike weights) is a kind the engine does not run.
- **Two doubled lead takes.** No public model is trained to pull one double from another. The nearest are a duet model (one re-upload, 630 MB, no licence, 7.9 dB on its own test) and a research model for duets and unisons (Conv-TasNet, CC BY 4.0) that needs its own runner. Neither is good enough to ship. Removing a double (keeping one dry lead) exists as a non-commercial model.
- **Soprano, alto, tenor and bass.** The good choir separator is online-only. The downloadable ones (rbustos89, SCNet, 42 MB) are undocumented and unlicensed, and of a kind the engine does not run.
- **Band splitting by frequency.** No public model outputs frequency bands; the band-split front ends split internally. The Frequency Band Splitter planned in the backlog covers this with filters.

## Licences in one place

- Permissive: the Demucs family (MIT), ZFTurbo's checkpoints (MIT repository), Kim's Mel-Band model (MIT), chenCFD guitar (MIT), the X-LANCE fine-tunes (MIT), Bandit v2 weights (CC BY-SA 4.0).
- Non-commercial: gilliaan's models, Sucial's chorus model, the Timbrica drum model (all CC BY-NC-SA 4.0), jaCappella (CC BY-NC 4.0).
- Copyleft: anvuew's karaoke model (GPL-3.0).
- No licence stated (all rights reserved by default; the list says so, and the user chooses): BS-RoFormer SW, becruily's guitar and karaoke models, Gabox, Gonzaluigi, the aufr33 and viperx karaoke model, the MDX23C drum model, the bleed suppressor, the duet model, rbustos89's choir models.

The full survey, with sizes, sources and scores per model, is kept with the project lead (it names the services and leaderboards it drew on, which this document does not).
