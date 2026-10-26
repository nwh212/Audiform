// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The web view's own right-click menu (Back, Refresh, Print, Share, in the
 * system language) has no place in an app. It is turned off everywhere except
 * in text fields, where Cut, Copy and Paste are still useful. The app's own
 * menus call preventDefault themselves, so they are not affected.
 */
export function wantsNativeMenu(target: EventTarget | null): boolean {
  const el = target as {
    tagName?: string;
    isContentEditable?: boolean;
    type?: string;
  } | null;
  if (!el?.tagName) return false;
  if (el.isContentEditable) return true;
  if (el.tagName === "TEXTAREA") return true;
  if (el.tagName !== "INPUT") return false;
  return ![
    "button",
    "checkbox",
    "radio",
    "range",
    "color",
    "file",
    "submit",
    "reset",
    "image",
  ].includes(el.type ?? "text");
}

export function installNativeMenuBlock(doc: Document = document): void {
  doc.addEventListener("contextmenu", (e) => {
    if (!wantsNativeMenu(e.target)) e.preventDefault();
  });
}
