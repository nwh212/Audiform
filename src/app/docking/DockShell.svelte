<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Hosts the docking layout.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import type { DockController } from "./dock";

  interface Props {
    dock: DockController;
  }

  const { dock }: Props = $props();
  let host: HTMLDivElement;

  onMount(() => {
    void dock.attach(host);
    return () => dock.dispose();
  });
</script>

<div class="dock" bind:this={host}></div>

<style>
  .dock {
    width: 100%;
    height: 100%;
    min-height: 0;
  }
</style>
