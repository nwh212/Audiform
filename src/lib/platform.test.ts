// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { detectPlatform, fileManagerName, pathsIgnoreCase, revealLabel } from "./platform";

describe("platform", () => {
  it("reads the webview user agent", () => {
    expect(detectPlatform("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Edg/141.0")).toBe("windows");
    expect(detectPlatform("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15")).toBe("linux");
    expect(detectPlatform("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15")).toBe("macos");
  });

  it("names things the way each system does", () => {
    expect(revealLabel("windows")).toBe("Show in Explorer");
    expect(revealLabel("linux")).toBe("Show in File Manager");
    expect(revealLabel("macos")).toBe("Reveal in Finder");
    expect(fileManagerName("windows")).toBe("File Explorer");
  });

  it("treats path case as significant only on Linux", () => {
    expect(pathsIgnoreCase("windows")).toBe(true);
    expect(pathsIgnoreCase("macos")).toBe(true);
    expect(pathsIgnoreCase("linux")).toBe(false);
  });
});
