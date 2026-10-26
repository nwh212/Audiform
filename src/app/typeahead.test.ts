// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { isTypingKey, typeahead } from "./typeahead";

const rows = ["Amplify", "Auto-Tune Pro", "Band EQ", null, "Reverb", "Razor", "Reverse"];

describe("type to jump", () => {
  it("jumps to the first row starting with the letter", () => {
    expect(typeahead().next("b", rows, -1, 0)).toBe(2);
  });

  it("builds a prefix from quick letters", () => {
    const t = typeahead();
    expect(t.next("r", rows, -1, 0)).toBe(4);
    expect(t.next("a", rows, 4, 100)).toBe(5);
  });

  it("steps through rows with the same letter when it is typed again", () => {
    const t = typeahead();
    expect(t.next("a", rows, -1, 0)).toBe(0);
    expect(t.next("a", rows, 0, 100)).toBe(1);
    expect(t.next("a", rows, 1, 200)).toBe(0);
  });

  it("starts over after a pause", () => {
    const t = typeahead();
    expect(t.next("r", rows, -1, 0)).toBe(4);
    expect(t.next("b", rows, 4, 5_000)).toBe(2);
  });

  it("skips rows that cannot be picked and says when nothing matches", () => {
    const t = typeahead();
    expect(t.next("z", rows, -1, 0)).toBeNull();
  });

  it("only takes plain typing keys", () => {
    expect(isTypingKey({ key: "a", ctrlKey: false, altKey: false, metaKey: false })).toBe(true);
    expect(isTypingKey({ key: "a", ctrlKey: true, altKey: false, metaKey: false })).toBe(false);
    expect(isTypingKey({ key: " ", ctrlKey: false, altKey: false, metaKey: false })).toBe(false);
    expect(isTypingKey({ key: "ArrowDown", ctrlKey: false, altKey: false, metaKey: false })).toBe(false);
  });
});
