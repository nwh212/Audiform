<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  A panel whose code loads the first time it is shown, so panels that
  are hidden by default do not slow down starting the app. Every other
  prop goes to the panel.
-->
<script lang="ts">
  import type { Component } from "svelte";

  interface Props {
    load: () => Promise<{ default: unknown }>;
    [prop: string]: unknown;
  }

  const { load, ...rest }: Props = $props();
  const loading = $derived(load().then((m) => m.default as Component<Record<string, unknown>>));
</script>

{#await loading then Panel}
  <Panel {...rest} />
{/await}
