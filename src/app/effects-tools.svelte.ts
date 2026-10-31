// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// Copyright (c) The Audiform contributors.
// This file is part of Audiform, based on Altered Audio Editor.

/**
 * The Effects menu: adding effects to the selected clip or track, Capture
 * Noise Print, Loudness Match, hiding effect windows and the plug-in
 * network prompt.
 */

import { PLATFORM } from "../lib/platform";
import { chainOf, newSlot, addSlot } from "../panels/voice-chain/chain";
import { effectWindowRequest, HIDE_EFFECT_WINDOWS } from "../panels/voice-chain/effect-window";
import { sessionHistory } from "../stores/history";
import { session } from "../stores/open-session";
import { prefs } from "../stores/prefs";
import { type EffectChainDoc } from "../stores/session";
import { settings } from "../stores/settings";
import { wave } from "../stores/wave";
import { type LearnSource, learnNoisePrint, effectsCatalog, firewallStatus, firewallBlock, pluginCloseWindow } from "./core";
import type { ActionRegistry } from "./actions";
import type { DockController } from "./docking/dock";
import type { ChoiceButton } from "./ChoiceDialog.svelte";
import type { GenerateMode } from "./GenerateDialog.svelte";
import type { MatchItem, MatchRow } from "./loudness-match";
import type { EffectTarget } from "../stores/session";
import type { EffectInfo } from "./core";

/** What the Effects menu needs from the rest of the app. */
export interface EffectsToolsDeps {
  actions: ActionRegistry;
  showNotice: (text: string) => void;
  showError: (e: unknown) => void;
  ask: (title: string, message: string, buttons: ChoiceButton[]) => Promise<string>;
  uuid: () => string;
  inWave: () => boolean;
  dock: DockController;
  fxTarget: () => { target: EffectTarget; chain: EffectChainDoc | undefined; name: string; wave?: boolean } | null;
  addEffectFromMenu: (effect: string) => Promise<void>;
  openLoudnessMatch: () => void;
  KEY_FIREWALL_OFFERED: string;
  fxCatalog: () => EffectInfo[] | null;
  set_fxCatalog: (v: EffectInfo[] | null) => void;
  lm: () => { mode: "clips" | "tracks"; items: MatchItem[]; rows: MatchRow[] | null; progress: number | null; target: string; region: string } | null;
  set_lm: (v: { mode: "clips" | "tracks"; items: MatchItem[]; rows: MatchRow[] | null; progress: number | null; target: string; region: string } | null) => void;
  exporting: () => { title: string; progress: number; floating?: boolean } | null;
  set_exporting: (v: { title: string; progress: number; floating?: boolean } | null) => void;
  generateMode: () => GenerateMode | null;
  set_generateMode: (v: GenerateMode | null) => void;
  pluginManagerOpen: () => boolean;
  set_pluginManagerOpen: (v: boolean) => void;
}

export function createEffectsTools(deps: EffectsToolsDeps) {
  const { actions, showNotice, showError, ask, uuid, inWave, dock, fxTarget, addEffectFromMenu, openLoudnessMatch, KEY_FIREWALL_OFFERED, fxCatalog, lm, exporting, generateMode, pluginManagerOpen, set_fxCatalog, set_lm, set_exporting, set_generateMode, set_pluginManagerOpen } = deps;

  // Effects menu: adds an effect to the Audio FX chain of the one selected
  // clip, or else the selected track, and shows the panel.
  const FX_MENU = [
    "vox_strip",
    "stereo_width",
    "dynamics",
    "limiter",
    "breath_gate",
    "deesser",
    "speech_leveler",
    "parametric_eq",
    "hum_cut",
    "gain",
    "noise_reducer",
    "noise_print",
    "sound_eraser",
    "hiss_reducer",
    "room_reducer",
    "click_repair",
    "clip_repair",
    "dc_offset",
    "band_eq_10",
    "curve_eq",
    "precision_filter",
    "notch",
    "pitch_shift",
    "center_extractor",
    "reverb",
    "channel_mixer",
    "echo",
    "phase_curve",
  ];

  /**
   * Capture Noise Print: learns the noise in the time selection and puts
   * it on a Noise Print Reducer in the effects of the selected clip or
   * track (updating the one already there). One undoable step.
   */
  async function captureNoisePrint() {
    const sel = noiseSelection();
    const at = fxTarget();
    if (!sel || !at) {
      showNotice("Select a stretch of only the noise (a pause or the room before anyone speaks), then choose Capture Noise Print.");
      return;
    }
    const source: LearnSource = at.wave || at.target.kind === "master" || at.target.kind === "bus"
      ? { kind: "wave", start: sel.start, end: sel.end }
      : { kind: at.target.kind, id: at.target.id, start: sel.start, end: sel.end };
    try {
      const learned = await learnNoisePrint(source);
      const catalog = fxCatalog() ?? (await effectsCatalog());
      set_fxCatalog(catalog);
      const info = catalog.find((e) => e.id === "altaudio.noise_print");
      if (!info) return;
      const chain = chainOf(at.chain);
      const existing = chain.slots.find((x) => x.effect === info.id);
      let next: EffectChainDoc | null;
      let slotId: string;
      if (existing) {
        slotId = existing.id;
        next = { ...chain, slots: chain.slots.map((x) => (x.id === existing.id ? { ...x, params: { ...x.params, ...learned.params }, preset: null } : x)) };
      } else {
        const slot = newSlot(info, uuid());
        slotId = slot.id;
        next = addSlot(chain, { ...slot, params: { ...slot.params, ...learned.params } });
        if (!next) {
          showNotice("That effects rack is full: it holds 16 effects.");
          return;
        }
      }
      const op = { op: "set_effects", target: at.target, chain: next } as const;
      if (at.wave) await wave.apply(op);
      else await session.edit("Capture Noise Print", op, sessionHistory);
      if (settings.current.openChainOnAdd) dock.show("voice-chain");
      effectWindowRequest.set(slotId);
      showNotice(`Learned the noise from ${learned.seconds.toFixed(2)} s and set it on ${at.name}.`);
    } catch (e) {
      showError(e);
    }
  }

  /** The time selection that Capture Noise Print learns from. */
  const noiseSelection = () => {
    const v = inWave() ? wave.current?.view.selection : session.current?.view.selection;
    return v && v.end > v.start ? v : null;
  };
  /**
   * Some plug-ins listen for connections, and Windows then asks whether
   * Audiform may use public networks. A firewall rule blocking incoming
   * connections for Audiform answers ahead of time. `quiet` is the
   * one-time offer at startup: it says nothing when all is well.
   */
  async function offerFirewallRule(quiet: boolean) {
    const s = await firewallStatus();
    if (!s.supported) return;
    if (s.covered) {
      if (!quiet) showNotice("Windows already blocks incoming connections for Audiform, so plug-ins should not make it ask about networks.");
      return;
    }
    prefs.set(KEY_FIREWALL_OFFERED, true);
    const pick = await ask(
      "Stop Network Prompts From Plug-ins",
      "Some plug-ins listen for connections (licence checks, or plug-ins of one maker finding each other), and Windows then asks whether Audiform may use public networks. Audiform itself never uses the network.\n\nAudiform can tell Windows to block incoming connections for it, so the question never comes up. Plug-ins on this computer still reach each other. Windows asks for administrator permission once.",
      [
        { label: "Not Now", value: "no" },
        { label: "Block and Stop Asking", value: "yes", primary: true },
      ],
    );
    if (pick !== "yes") return;
    try {
      const after = await firewallBlock();
      showNotice(after.covered ? "Done: Windows now blocks incoming connections for Audiform and will not ask again." : "The rule could not be confirmed. Try Effects > Stop Network Prompts From Plug-ins again.");
    } catch (e) {
      showError(e);
    }
  }

  /** Registers the menu actions. */
  function register(): (() => void)[] {
    return [
      ...FX_MENU.map((short) =>
        actions.register({
          id: `fx.add.${short}`,
          run: () => addEffectFromMenu(`altaudio.${short}`),
          enabled: () => fxTarget() !== null,
          reason: () => (inWave() ? "Open a file in the Waveform editor first." : "Select a track or one clip first."),
        }),
      ),
      actions.register({
        id: "fx.loudness-match",
        run: () => openLoudnessMatch(),
        enabled: () => exporting() === null && lm() === null && (session.current?.tracks.some((t) => t.clips.length > 0) ?? false),
        reason: () => "The session has no clips yet.",
      }),
      actions.register({ id: "fx.firewall", run: () => offerFirewallRule(false), visible: () => PLATFORM === "windows" }),
      actions.register({
        id: "fx.capture-noise-print",
        run: () => captureNoisePrint(),
        enabled: () => noiseSelection() !== null && fxTarget() !== null,
        reason: () => "Select a stretch of only the noise, and a track or clip to put the print on.",
      }),
      actions.register({
        id: "fx.hide-windows",
        run: () => {
          window.dispatchEvent(new Event(HIDE_EFFECT_WINDOWS));
          // Plug-ins' own windows too.
          const d = session.current;
          if (!d) return;
          const chains = [...d.tracks.flatMap((t) => [t.effects, ...t.clips.map((c) => c.effects)]), ...(d.buses ?? []).map((b) => b.effects), d.master?.effects];
          for (const ch of chains) for (const s of ch?.slots ?? []) if (s.effect.startsWith("vst3:")) void pluginCloseWindow(s.id).catch(() => {});
          for (const t of d.tracks) if (t.instrument?.effect.startsWith("vst3:")) void pluginCloseWindow(t.instrument.id).catch(() => {});
        },
      }),
      actions.register({
        id: "fx.gen.tone",
        run: () => {
          set_generateMode("tone");
        },
        enabled: () => !inWave() && session.current !== null,
        reason: () => "Generate works in the Multitrack editor.",
      }),
      actions.register({
        id: "fx.gen.noise",
        run: () => {
          set_generateMode("noise");
        },
        enabled: () => !inWave() && session.current !== null,
        reason: () => "Generate works in the Multitrack editor.",
      }),
      actions.register({
        id: "fx.gen.silence",
        run: () => {
          set_generateMode("silence");
        },
        enabled: () => !inWave() && (session.current?.tracks.length ?? 0) > 0,
        reason: () => "Insert Silence works in the Multitrack editor.",
      }),
      actions.register({
        id: "fx.plugin-manager",
        run: () => {
          set_pluginManagerOpen(true);
        },
      }),
    ];
  }

  return { register, offerFirewallRule };
}
