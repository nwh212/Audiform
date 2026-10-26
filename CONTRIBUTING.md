# Contributing to Audiform

Thanks for helping. Bug fixes, small features, tests, translations of docs and hardware testing are all welcome. You do not need to be an expert, and you do not need to know Git well to start.

## Start here

1. Open the [Roadmap](ROADMAP.md) or the Issues tab and look for the labels **good first issue** and **help wanted**.
2. Comment on the issue ("I'd like to take this") so two people don't build the same thing.
3. No code? Testing counts. Issues labeled **needs hardware testing** are for people with a specific interface, a Mac or Linux machine.

## How changes get in

1. Fork the repo and make a branch for your change.
2. Keep each pull request to one fix or feature.
3. Run the checks (the same ones CI runs): `npm run check`, `npm test`, `cargo fmt --check`, `cargo clippy`, `cargo test` from the README. They must pass.
4. Open a pull request. The template has a checklist; fill it in and describe how you tested.

Audiform is maintained by Broman ([nwh212](https://github.com/nwh212)), who reviews and merges pull requests. Bigger features: open an issue first so we can agree on the approach before you spend the time.

## Ground rules

- **Original work only.** Don't copy code, icons, artwork, names or layouts from commercial audio software, and don't paste code you don't have the right to share. Describe features by what they do, not by what another product calls them.
- **No other products' names** in text, code, comments, issues or pull requests. A check in CI and `npm run check` looks for these.
- **Privacy stays absolute.** No network calls, telemetry, accounts or tracking. The CI privacy check enforces this.
- **Every source file** starts with the GPL header used in the existing files.
- **Plain UI text:** short sentences. Controls for features that don't exist yet show disabled with the reason.
- **Keyboard shortcuts** are not added or changed without the maintainer's approval.
- **Audio code:** nothing in the audio callback may allocate, lock or wait. Ask in the issue if unsure.

## Giving back to Altered Audio Editor

Audiform is a fork of [Altered Audio Editor](https://github.com/geekcastradio/altered-audio-editor) by Steve Phillips and The GeekCast Radio Network, LLC, and it stays easy for them to take anything from Audiform. So that they can merge it, everything contributed to Audiform is also offered to The GeekCast Radio Network, LLC under the terms of Altered Audio Editor's [Contributor License Agreement](CLA.md). By opening a pull request here, you agree to that. In short: you keep the copyright on your work, it is released under GPL v3.0 or later, and The GeekCast Radio Network, LLC may use it in Altered Audio Editor under the terms of that agreement.

To keep changes easy to carry over: one feature per commit, the same folder layout, and a line in `docs/decisions.md` for each choice.

## Reporting bugs

Use the bug report form in [Audiform's Issues](https://github.com/nwh212/audiform/issues). Please report Audiform problems here, not to the Altered Audio Editor project. Include your OS and version, the app version (Help > About), what you did, what you expected and what happened. Security problems go through [SECURITY.md](SECURITY.md), not public issues.

## Name and logo

The code is GPL. Names and logos are not: "Altered Audio Editor" and its logo belong to The GeekCast Radio Network, LLC, and Audiform has its own. See [TRADEMARKS.md](TRADEMARKS.md). Forks of Audiform are welcome under a different name.
