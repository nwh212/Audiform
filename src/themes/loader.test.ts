// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { applyThemeId, findTheme, installTheme, isAvailable, listThemes, resolveCss, resolvePointers, resolveThemeId, themeLook, useThemeSource, type ThemeSource } from "./loader";
import { validateTheme } from "./validate";

const PIXEL = "data:image/png;base64,iVBORw0KGgo=";

function file(id: string, over: Record<string, unknown> = {}): string {
  return JSON.stringify({
    format: "audiform-theme",
    version: 1,
    id,
    name: id.toUpperCase(),
    base: "dark",
    css: `:root[data-theme="${id}"] { --accent: red; }`,
    assets: {},
    ...over,
  });
}

/** Fake built-in and user files. */
function fake(builtin: Record<string, string>, user: Record<string, string> = {}): ThemeSource {
  return {
    builtin: () => Object.fromEntries(Object.entries(builtin).map(([name, text]) => [`./${name}`, () => Promise.resolve(text)])),
    userList: () => Promise.resolve(Object.keys(user).map((fileName) => ({ fileName, size: user[fileName]!.length }))),
    userRead: (name) => (name in user ? Promise.resolve(user[name]!) : Promise.reject(new Error("gone"))),
  };
}

/** The few parts of a page the loader touches (the tests run without a browser). */
class FakeEl {
  id = "";
  textContent = "";
  constructor(private readonly owner: FakeDoc) {}
  remove(): void {
    this.owner.els = this.owner.els.filter((e) => e !== this);
  }
}
class FakeDoc {
  els: FakeEl[] = [];
  documentElement = { dataset: {} as Record<string, string | undefined> };
  head = { appendChild: (e: FakeEl) => void this.els.push(e) };
  getElementById(id: string): FakeEl | null {
    return this.els.find((e) => e.id === id) ?? null;
  }
  createElement(): FakeEl {
    return new FakeEl(this);
  }
  get styles(): FakeEl[] {
    return this.els.filter((e) => e.id === "audiform-theme");
  }
  get asDocument(): Document {
    return this as unknown as Document;
  }
}

let page = new FakeDoc();
beforeEach(() => {
  useThemeSource(null);
  page = new FakeDoc();
});
afterEach(() => useThemeSource(null));

describe("theme list", () => {
  it("lists built-in themes, then the user's, each by name", async () => {
    useThemeSource(fake({ "zeta.audtheme": file("zeta"), "alpha.audtheme": file("alpha") }, { "mine.audtheme": file("mine"), "also.audtheme": file("also") }));
    const c = await listThemes([]);
    expect(c.themes.map((t) => [t.id, t.builtin])).toEqual([
      ["alpha", true],
      ["zeta", true],
      ["also", false],
      ["mine", false],
    ]);
    expect(c.problems).toEqual([]);
  });

  it("leaves a bad file out with a plain message and keeps going", async () => {
    useThemeSource(fake({ "ok.audtheme": file("ok"), "broken.audtheme": "{" }, { "bad.audtheme": file("bad", { base: "grey" }), "good.audtheme": file("good"), "gone.audtheme": file("gone") }));
    const c = await listThemes([]);
    expect(c.themes.map((t) => t.id)).toEqual(["ok", "gone", "good"]);
    expect(c.problems.map((p) => p.file).sort()).toEqual(["bad.audtheme", "broken.audtheme"]);
    expect(c.problems.find((p) => p.file === "bad.audtheme")?.message).toContain("base");
  });

  it("reports a file that cannot be read", async () => {
    const s = fake({}, { "a.audtheme": file("a") });
    s.userRead = () => Promise.reject(new Error("no access"));
    useThemeSource(s);
    const c = await listThemes([]);
    expect(c.themes).toEqual([]);
    expect(c.problems[0]?.message).toContain("no access");
  });

  it("refuses a user theme that reuses an id", async () => {
    useThemeSource(fake({ "meadow.audtheme": file("meadow") }, { "copy.audtheme": file("meadow"), "one.audtheme": file("same"), "two.audtheme": file("same") }));
    const c = await listThemes([]);
    expect(c.themes.map((t) => t.file)).toEqual(["meadow.audtheme", "one.audtheme"]);
    expect(c.problems.map((p) => p.file).sort()).toEqual(["copy.audtheme", "two.audtheme"]);
    expect(c.problems[0]?.message).toContain("already used");
  });

  it("refuses a built-in file whose name does not match its id", async () => {
    useThemeSource(fake({ "one.audtheme": file("two") }));
    const c = await listThemes([]);
    expect(c.themes).toEqual([]);
    expect(c.problems[0]?.message).toContain("file name");
  });

  it("survives a Themes folder that cannot be read", async () => {
    const s = fake({ "a.audtheme": file("a") });
    s.userList = () => Promise.reject(new Error("no folder"));
    useThemeSource(s);
    expect((await listThemes([])).themes.map((t) => t.id)).toEqual(["a"]);
  });

  it("looks at the folder again on request", async () => {
    const user: Record<string, string> = {};
    useThemeSource(fake({}, user));
    expect((await listThemes([])).themes).toEqual([]);
    user["new.audtheme"] = file("new");
    expect((await listThemes([])).themes).toEqual([]);
    expect((await listThemes([], true)).themes.map((t) => t.id)).toEqual(["new"]);
  });
});

describe("secret themes", () => {
  const s = () => fake({ "plain.audtheme": file("plain"), "hidden.audtheme": file("hidden", { secret: true }) }, { "mine.audtheme": file("quiet", { secret: true }) });

  it("are never listed, unlocked or not", async () => {
    useThemeSource(s());
    expect((await listThemes([])).themes.map((t) => t.id)).toEqual(["plain"]);
    expect((await listThemes(["hidden", "quiet"])).themes.map((t) => t.id)).toEqual(["plain"]);
    expect((await listThemes([])).secretIds.sort()).toEqual(["hidden", "quiet"]);
  });

  it("cannot be used by id until unlocked", async () => {
    useThemeSource(s());
    expect(await findTheme("hidden", [])).toBeNull();
    expect(await findTheme("quiet", [])).toBeNull();
    expect((await findTheme("hidden", ["hidden"]))?.id).toBe("hidden");
    expect(await resolveThemeId("hidden", [])).toBe("dark");
    expect(await resolveThemeId("hidden", ["hidden"])).toBe("hidden");
  });

  it("follow the flag", () => {
    expect(isAvailable({ id: "a", secret: true }, [])).toBe(false);
    expect(isAvailable({ id: "a", secret: true }, ["a"])).toBe(true);
    expect(isAvailable({ id: "a" }, [])).toBe(true);
  });
});

describe("id fallback", () => {
  it("uses dark for unknown, broken and missing themes", async () => {
    useThemeSource(fake({ "ok.audtheme": file("ok"), "broken.audtheme": "nope" }));
    expect(await resolveThemeId("ok", [])).toBe("ok");
    expect(await resolveThemeId("dark", [])).toBe("dark");
    expect(await resolveThemeId("nothing", [])).toBe("dark");
    expect(await resolveThemeId("broken", [])).toBe("dark");
  });

  it("finds a user theme by id", async () => {
    useThemeSource(fake({}, { "mine.audtheme": file("mine") }));
    expect(await resolveThemeId("mine", [])).toBe("mine");
  });
});

describe("applying a theme", () => {
  it("writes one style element with the assets in place and sets data-theme", async () => {
    useThemeSource(fake({ "pic.audtheme": file("pic", { css: ':root[data-theme="pic"] body { background: url("asset:bg") #000; }', assets: { bg: PIXEL } }) }));
    expect(await applyThemeId("pic", [], page.asDocument)).toBe("pic");
    expect(page.documentElement.dataset.theme).toBe("pic");
    const styles = page.styles;
    expect(styles).toHaveLength(1);
    expect(styles[0]?.textContent).toContain(`url("${PIXEL}")`);
    expect(styles[0]?.textContent).not.toContain("asset:");
    await applyThemeId("pic", [], page.asDocument);
    expect(page.styles).toHaveLength(1);
  });

  it("goes back to the built-in look for dark or an unknown id", async () => {
    useThemeSource(fake({ "a.audtheme": file("a") }));
    await applyThemeId("a", [], page.asDocument);
    expect(await applyThemeId("missing", [], page.asDocument)).toBe("dark");
    expect(page.documentElement.dataset.theme).toBeUndefined();
    expect(page.getElementById("audiform-theme")).toBeNull();
    await applyThemeId("a", [], page.asDocument);
    expect(await applyThemeId("dark", [], page.asDocument)).toBe("dark");
    expect(page.getElementById("audiform-theme")).toBeNull();
  });

  it("lets only the latest request win", async () => {
    useThemeSource(fake({ "a.audtheme": file("a"), "b.audtheme": file("b") }));
    const first = applyThemeId("a", [], page.asDocument);
    const second = applyThemeId("b", [], page.asDocument);
    await Promise.all([first, second]);
    expect(page.documentElement.dataset.theme).toBe("b");
  });

  it("puts light themes in a light color scheme first and keeps the look flags", async () => {
    const t = validateTheme(file("lite", { base: "light", canvasLook: { pixel: true } }));
    expect(t.ok && resolveCss(t.theme).startsWith(':root[data-theme="lite"] { color-scheme: light; }')).toBe(true);
    useThemeSource(fake({ "lite.audtheme": file("lite", { base: "light", canvasLook: { pixel: true } }) }));
    await applyThemeId("lite", [], page.asDocument);
    expect(themeLook().pixel).toBe(true);
    installTheme(null, page.asDocument);
    expect(themeLook()).toEqual({});
  });

  it("turns pointers into ready pictures", () => {
    const t = validateTheme(file("pt", { assets: { p: PIXEL }, pointers: { text: { asset: "p", hot: [7, 10] } } }));
    expect(t.ok && resolvePointers(t.theme)).toEqual({ text: { url: PIXEL, hot: [7, 10] } });
    const none = validateTheme(file("nopt"));
    expect(none.ok && resolvePointers(none.theme)).toBeNull();
  });
});

describe("shipped themes", () => {
  it("all pass the check, match their file names and cover the old themes", async () => {
    const c = await listThemes([]);
    expect(c.problems).toEqual([]);
    expect(c.themes.map((t) => t.id).sort()).toEqual(["analog", "aqua", "mallnight", "mallpastel", "meadow", "moonlight", "phosphor", "prism"]);
    for (const t of c.themes) {
      expect(t.css).toContain(`:root[data-theme="${t.id}"]`);
      expect(t.css).not.toMatch(new RegExp(`data-theme="(?!${t.id}")`));
    }
  });

  it("carry the look flags the code used to get from theme names", async () => {
    const phosphor = await findTheme("phosphor", []);
    const prism = await findTheme("prism", []);
    const analog = await findTheme("analog", []);
    expect(phosphor?.canvasLook).toMatchObject({ pixel: true, markCursor: true });
    expect(prism?.canvasLook).toMatchObject({ markCursor: true });
    expect(analog?.canvasLook?.markCursor).toBeFalsy();
  });
});
