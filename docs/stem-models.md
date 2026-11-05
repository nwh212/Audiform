# Stem models: what Audio Separation offers and why

Audiform's Audio Separation panel downloads models on demand (see `src-tauri/src/separation/mod.rs` for the list, each with a pinned checksum). The engine runs the two band-split RoFormer families; models of other kinds cannot be added until the engine learns them. This page records the survey behind the list, so the next check (every major release, `docs/RELEASING.md` step 2) starts from here. Surveyed 2026-11-03, extended 2026-11-04 (Mega 53, duet, more of gilliaan's models) and 2026-11-05 (the rest of the X-LANCE fine-tunes).

## What ships

| Need | Model in the list | Why this one |
| --- | --- | --- |
| Vocals and instrumental | BS PolarFormer, Mel-Band Roformer (Kim), Mel-Band Roformer Deux, BS-RoFormer (anvuew) | Best public scores on the common 100-song test, permissive or freely published. anvuew's is GPL-3.0 and scored 12.55 dB on the trainer's own test, which is not the same test. The list folds models that make the same split under one heading, so more of a kind can be offered. |
| Six stems at once (bass, drums, other, vocals, guitar, piano) | BS-RoFormer SW | Best public scores on every one of those stems. Its provenance is disputed and no licence is stated, which the list says. |
| Bass, drums, guitar, keys, orchestra, percussion, synth and vocals on their own | X-LANCE fine-tunes | MIT licence; each fine-tuned from the six-stem SW model, so the best of the SW family for one instrument. Where X-LANCE made two versions (drums, orchestra, percussion, synth) the list has the second. No published scores. |
| Guitar, trained for it | Mel-Band Roformer Guitar (chenCFD) | MIT licence, small (277 MB), a published score of 7.98 dB on the trainer's own test. |
| Lead vocal apart from backing vocals | BS-RoFormer Karaoke (frazer and becruily), and anvuew's | The two best public lead-vocal scores (10.10 and 10.22 dB) among single models. The first has no licence stated; anvuew's is GPL-3.0. Run the vocals model first, then one of these on the vocal stem, to get the backing vocals as the rest. |
| Bowed strings | BS-RoFormer Bowed Strings v2 (gilliaan) | The only public strings model, 4.69 dB on a public strings test. CC BY-NC-SA 4.0: free for non-commercial use, with credit, shared alike. |
| Everything at once (53 instruments) | Mega 53 (ZFTurbo) | The only public model that names 53 instruments, MIT-licensed training code. 1.37 GB and heavy: 53 results per run and a large graphics card. Best used to find what is in a mix. |
| One instrument out of the 53, on its own | Mega 53 splits (noblebarkrr) | The same model cut into one 78 MB model per instrument, so you download and run only the one you need. Listed under one folded heading. |
| Two singers apart | Duet (DryPaintMan) | Pulls two voices singing together apart. 7.93 dB on the trainer's test; no licence stated. |
| Lead vocal, drums, effects off vocals, cleaner inverted vocals | gilliaan's karaoke, drums, de-effect (beta) and clean-invert models | CC BY-NC-SA 4.0. De-effect takes reverb and delay off a vocal; clean-invert tidies a vocal made by subtracting an instrumental from a mix. |
| Center and wide (stereo field) | gilliaan's CenterWide, Mel-Band v3 and BS v3 beta | A learned split of what sits in the phantom center from what is panned off it, on its own Stereo tab. CC BY-NC-SA 4.0. |
| Noise | Mel-Band Roformer Denoise, and the aggressive one | As before. |
| Reverb | Reverb Remover (anvuew) | Pulls the dry sound out of a reverberant recording, GPL-3.0, 22.5 dB on the trainer's own test. |
| Bleed between stems | Mel-Band Roformer Bleed Suppressor (unwa and 97chris) | Cleans what other stems left in an instrumental. No licence stated. |

## Looked at and left out, with the reason

- **Vocal and instrumental models with changed insides.** Several high-scoring vocal and instrumental models (pcunwa's HyperACE, Large-Inst and Revive series) change the network itself, not only its weights, so the engine cannot run them until it learns those changes. The small 46M-parameter four-stem models (HiDolen) need a different runner. becruily's separate vocals and instrumental models have no licence and no published score; Deux covers them.
- **De-limiter.** becruily's model that undoes a brickwall limiter (CC BY-NC 4.0, 102 MB) gives one output, not a split; the list shows splits only. A candidate once the panel has a place for one-output clean-up models.

- **Drum kit into kick, snare, toms, hi-hat and cymbals.** The best public model (an MDX23C by aufr33 and jarredou, 438 MB) is of a kind the engine does not run, its original release is gone and only mirrors remain, and no licence was ever stated. The permissive one (a Hybrid Demucs by inagoy, MIT) is also a kind the engine does not run and scores well below it. The best kit splitters of all are online-only services.
- **Synth lead, percussion.** The public models are re-uploads of unknown provenance or unverified links.
- **CenterWide on MDX23C and SCNet.** gilliaan's MDX23C CenterWide v2e is, by the trainer's account, the best of the CenterWide set (twice the training data, best score, least noise). It is an MDX23C network, which the engine does not run yet; the RoFormer versions ship meanwhile. The SCNet versions wait for the same reason.
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
