<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The GeekCast Radio Network, LLC.
  This file is part of Audiform, based on Altered Audio Editor.

  Body for a panel whose feature is not built yet. States that plainly
  instead of showing fake controls.
-->
<script lang="ts">
  interface Props {
    title: string;
    note: string;
  }

  const { title, note }: Props = $props();
</script>

<div class="pending">
  <p class="title">{title}</p>
  <p class="note">{note}</p>
</div>

<style>
  .pending {
    display: grid;
    place-content: center;
    gap: var(--space-2);
    height: 100%;
    padding: var(--space-4);
    text-align: center;
    background: var(--surface-1);
  }

  p {
    margin: 0;
  }

  .title {
    color: var(--text-dim);
  }

  .note {
    color: var(--text-faint);
    font-size: var(--font-size-small);
  }
</style>
