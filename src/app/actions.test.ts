// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { describe, expect, it } from "vitest";
import { get } from "svelte/store";
import { ActionRegistry } from "./actions";

describe("ActionRegistry", () => {
  it("treats unregistered ids as disabled", async () => {
    const reg = new ActionRegistry();
    expect(reg.isEnabled("file.open")).toBe(false);
    expect(await reg.run("file.open")).toBe(false);
  });

  it("runs enabled actions and skips disabled ones", async () => {
    const reg = new ActionRegistry();
    let count = 0;
    let on = true;
    reg.register({ id: "x", run: () => void count++, enabled: () => on });
    expect(await reg.run("x")).toBe(true);
    on = false;
    expect(await reg.run("x")).toBe(false);
    expect(count).toBe(1);
  });

  it("reports checked state and dynamic labels", () => {
    const reg = new ActionRegistry();
    reg.register({ id: "t", run: () => {}, checked: () => true, label: () => "Undo Add Track" });
    expect(reg.isChecked("t")).toBe(true);
    expect(reg.label("t")).toBe("Undo Add Track");
    expect(reg.isChecked("missing")).toBeUndefined();
  });

  it("signals changes on register, run and unregister", async () => {
    const reg = new ActionRegistry();
    const start = get(reg.changed);
    const off = reg.register({ id: "a", run: () => {} });
    await reg.run("a");
    off();
    expect(get(reg.changed)).toBe(start + 3);
    expect(reg.has("a")).toBe(false);
  });

  it("an overlay stands in while its condition holds", async () => {
    const reg = new ActionRegistry();
    const ran: string[] = [];
    reg.register({ id: "edit.delete", run: () => void ran.push("clips"), enabled: () => false });
    let wave = false;
    const off = reg.overlay(() => wave, { id: "edit.delete", run: () => void ran.push("wave"), label: () => "Delete Selection" });
    expect(reg.isEnabled("edit.delete")).toBe(false);
    wave = true;
    expect(reg.isEnabled("edit.delete")).toBe(true);
    expect(reg.label("edit.delete")).toBe("Delete Selection");
    await reg.run("edit.delete");
    off();
    expect(reg.label("edit.delete")).toBeNull();
    expect(ran).toEqual(["wave"]);
    reg.overlay(() => true, { id: "only.overlay", run: () => {} });
    expect(reg.has("only.overlay")).toBe(true);
  });
});
