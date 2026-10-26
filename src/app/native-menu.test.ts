// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { wantsNativeMenu } from "./native-menu";

describe("native right-click menu", () => {
  it("stays only in text fields", () => {
    expect(
      wantsNativeMenu({
        tagName: "INPUT",
        type: "text",
      } as unknown as EventTarget),
    ).toBe(true);
    expect(
      wantsNativeMenu({ tagName: "TEXTAREA" } as unknown as EventTarget),
    ).toBe(true);
    expect(
      wantsNativeMenu({
        tagName: "DIV",
        isContentEditable: true,
      } as unknown as EventTarget),
    ).toBe(true);
    expect(
      wantsNativeMenu({
        tagName: "INPUT",
        type: "range",
      } as unknown as EventTarget),
    ).toBe(false);
    expect(wantsNativeMenu({ tagName: "DIV" } as unknown as EventTarget)).toBe(
      false,
    );
    expect(wantsNativeMenu(null)).toBe(false);
  });
});
