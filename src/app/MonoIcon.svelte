<!--
  SPDX-License-Identifier: GPL-3.0-or-later
  Copyright (c) The Audiform contributors.
  This file is part of Audiform, based on Altered Audio Editor.

  The mono icon: two channels (left and right) joining into one.
-->
<script lang="ts">
  // No props: the icon takes its color from the button it sits in.
</script>

<svg class="mono-icon" viewBox="0 0 16 16" aria-hidden="true">
  <path d="M3.2 2.5 Q3.2 7.2 8 9.2 Q12.8 7.2 12.8 2.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
  <path d="M8 9.2 V13.6" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" />
  <circle cx="8" cy="9.2" r="1.5" fill="currentColor" />
</svg>

<style>
  .mono-icon {
    display: block;
    width: 13px;
    height: 13px;
    margin: auto;
  }
</style>
