// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { DEFAULT_SETTINGS, sanitizeSettings } from "../stores/settings";
import { shouldNotify } from "./desktop-notify";

describe("desktop notifications", () => {
  it("only on Linux, only when on, only with the window in the background", () => {
    expect(shouldNotify("linux", true, false)).toBe(true);
    expect(shouldNotify("linux", true, true)).toBe(false);
    expect(shouldNotify("linux", false, false)).toBe(false);
    expect(shouldNotify("windows", true, false)).toBe(false);
    expect(shouldNotify("macos", true, false)).toBe(false);
  });

  it("is on by default and can be turned off", () => {
    expect(DEFAULT_SETTINGS.desktopNotifications).toBe(true);
    expect(sanitizeSettings({}).desktopNotifications).toBe(true);
    expect(sanitizeSettings({ desktopNotifications: false }).desktopNotifications).toBe(false);
  });
});
