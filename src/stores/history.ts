// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

import { UndoStack } from "../commands";

/**
 * Undo history for the open session.
 *
 * One session is open at a time in P1. When multiple open documents
 * arrive, each gets its own UndoStack and this becomes the stack of
 * the active document.
 */
export const sessionHistory = new UndoStack({ baseLabel: "New Session" });
