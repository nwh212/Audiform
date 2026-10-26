// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * How the Waveform editor's spectrum is shown: its frequency range and
 * scale, the labels (Hertz or note names) and the analysis resolution.
 * Kept for the run of the app, across files.
 */

import { writable } from "svelte/store";
import { FULL_VIEW, type FreqView } from "../panels/editor/spectrum";

export interface SpectrumViewState extends FreqView {
  /** Scale labels as note names (C4, A4) instead of Hz. */
  notes: boolean;
  /** Analysis frame length: longer resolves frequency finer, time coarser. */
  fft: number;
}

export const FFT_SIZES = [512, 1024, 2048, 4096, 8192, 16384] as const;

export const spectrumView = writable<SpectrumViewState>({ ...FULL_VIEW, notes: false, fft: 2048 });
